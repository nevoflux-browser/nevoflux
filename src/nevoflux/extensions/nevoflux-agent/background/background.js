/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

// Immediate debug log to verify script is loading
console.log("[NevoFlux] Background script starting...");

/**
 * NevoFlux Agent Background Script
 * Manages communication between:
 * - Chat Sidebar (Dioxus WASM) <-> Native Messaging Host (Rust)
 * - Chat Sidebar <-> Content Sidebar (Dioxus WASM in Shadow DOM)
 *
 * Protocol Version: 3.1 (Computer Use support)
 */

// Native messaging port to Rust agent
let nativePort = null;
let portListeners = null;

// Track Content Sidebar injection status per tab
const contentSidebarStatus = new Map();

// Track pending tool calls waiting for results
const pendingToolCalls = new Map();

// Message type constants matching shared-protocol
const MessageTypes = {
  // Chat Sidebar -> Native Agent
  CHAT_MESSAGE: "chat_message",
  STOP_GENERATION: "stop_generation",
  UI_ACTION: "ui_action",
  REQUEST_TAB_CONTEXT: "request_tab_context",

  // Native Agent -> Chat Sidebar
  STREAM_CHUNK: "stream_chunk",
  STREAM_END: "stream_end",
  AGENT_ERROR: "agent_error",
  TAB_CONTEXT_UPDATE: "tab_context_update",
  CONNECTION_STATUS: "connection_status",

  // Chat Sidebar -> Content Sidebar (via Background)
  DISPLAY_CONTENT: "display_content",
  CLEAR_CONTENT: "clear_content",
  HIGHLIGHT_ELEMENT: "highlight_element",
  CLEAR_HIGHLIGHT: "clear_highlight",

  // Content Sidebar -> Chat Sidebar (via Background)
  CONTENT_URL_REPORT: "content_url_report",
  CONTENT_ELEMENT_CLICK: "content_element_click",
  CONTENT_SIDEBAR_READY: "content_sidebar_ready",

  // Page Context (Computer Use)
  REQUEST_PAGE_CONTEXT: "request_page_context",
  PAGE_CONTEXT_RESPONSE: "page_context_response",

  // Tool Execution (Computer Use)
  TOOL_CALL: "tool_call",
  TOOL_RESULT: "tool_result",
  AGENT_STATE_UPDATE: "agent_state_update",

  // System
  PING: "ping",
  PONG: "pong",
  INJECT_CONTENT_SIDEBAR: "inject_content_sidebar",
  CONTENT_SIDEBAR_INJECTED: "content_sidebar_injected",
};

// Tools that are executed by Background Script (not Content Sidebar)
const BACKGROUND_TOOLS = ["screenshot", "navigate"];

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
    console.log("[NevoFlux] Native messaging already connected");
    return;
  }

  console.log("[NevoFlux] Attempting to connect to native agent...");

  try {
    console.log("[NevoFlux] Calling browser.runtime.connectNative('com.nevoflux.agent')...");
    nativePort = browser.runtime.connectNative("com.nevoflux.agent");
    console.log("[NevoFlux] connectNative returned, port:", nativePort);

    portListeners = {
      onMessage: (message) => {
        console.log("[NevoFlux] Received from native:", message);
        handleNativeMessage(message);
      },
      onDisconnect: (port) => {
        const errorMsg = port.error ? port.error.message || String(port.error) : "null";
        console.error("[NevoFlux] Native messaging disconnected:", errorMsg);
        console.error("[NevoFlux] Port error object:", port.error);
        console.error("[NevoFlux] Last error:", browser.runtime.lastError);
        nativePort = null;
        portListeners = null;

        // Notify sidebar of disconnection
        broadcastToSidebar({
          type: MessageTypes.CONNECTION_STATUS,
          payload: { connected: false, error: errorMsg }
        });
      }
    };

    console.log("[NevoFlux] Adding message and disconnect listeners...");
    nativePort.onMessage.addListener(portListeners.onMessage);
    nativePort.onDisconnect.addListener(portListeners.onDisconnect);

    // Send initial ping to native agent to verify connection
    const pingMessage = {
      type: "ping",
      payload: { timestamp: Date.now() }
    };
    console.log("[NevoFlux] Sending initial ping to native agent:", JSON.stringify(pingMessage));

    try {
      nativePort.postMessage(pingMessage);
      console.log("[NevoFlux] Initial ping sent successfully");
    } catch (postError) {
      console.error("[NevoFlux] Failed to send initial ping:", postError);
    }

    // Notify sidebar of connection
    broadcastToSidebar({
      type: MessageTypes.CONNECTION_STATUS,
      payload: { connected: true }
    });

    console.log("[NevoFlux] Native messaging connected");
  } catch (error) {
    console.error("[NevoFlux] Failed to connect native messaging:", error);
    console.error("[NevoFlux] Error stack:", error.stack);
    nativePort = null;

    broadcastToSidebar({
      type: MessageTypes.CONNECTION_STATUS,
      payload: { connected: false, error: error.message }
    });
  }
}

/**
 * Handle messages received from native agent
 */
function handleNativeMessage(message) {
  const msgType = message.type;

  // Route native agent messages
  switch (msgType) {
    case MessageTypes.STREAM_CHUNK:
    case MessageTypes.STREAM_END:
    case MessageTypes.AGENT_ERROR:
    case MessageTypes.TAB_CONTEXT_UPDATE:
      broadcastToSidebar(message);
      break;

    // Browser control messages need to be forwarded to content script
    case "browser_control":
      handleBrowserControlFromAgent(message.payload);
      break;

    // Tool calls from native agent need to go to Content Sidebar
    case MessageTypes.TOOL_CALL:
      console.log("[NevoFlux] Routing tool_call to Content Sidebar:", message.payload);
      // Also broadcast to Chat Sidebar for UI feedback
      broadcastToSidebar(message);
      // Execute the tool via Content Sidebar
      handleToolCall(message.payload);
      break;

    // Page context requests need to go to Content Sidebar
    case MessageTypes.REQUEST_PAGE_CONTEXT:
      console.log("[NevoFlux] Routing page context request to Content Sidebar");
      handleRequestPageContext(message.payload?.session_id);
      break;

    // Agent state updates go to Chat Sidebar for UI
    case MessageTypes.AGENT_STATE_UPDATE:
      broadcastToSidebar(message);
      break;

    default:
      console.log("[NevoFlux] Forwarding native message to sidebar:", msgType);
      broadcastToSidebar(message);
  }
}

/**
 * Broadcast message to sidebar (Chat Sidebar Dioxus app)
 */
function broadcastToSidebar(message) {
  browser.runtime.sendMessage(message).catch((err) => {
    // Sidebar might not be open - this is normal
    console.debug("[NevoFlux] Sidebar not available:", err.message);
  });
}

/**
 * Send message to Content Sidebar in a specific tab
 */
async function sendToContentSidebar(tabId, message) {
  try {
    await browser.tabs.sendMessage(tabId, {
      target: "content-sidebar",
      ...message
    });
  } catch (error) {
    console.warn(`[NevoFlux] Failed to send to Content Sidebar in tab ${tabId}:`, error);
  }
}

/**
 * Broadcast message to all Content Sidebars
 */
async function broadcastToContentSidebars(message) {
  const tabs = await browser.tabs.query({});
  for (const tab of tabs) {
    if (contentSidebarStatus.get(tab.id)) {
      sendToContentSidebar(tab.id, message);
    }
  }
}

/**
 * Handle messages from Chat Sidebar or Content Sidebar
 */
browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
  console.log("[NevoFlux] Background received:", message.type, message);

  const msgType = message.type;

  // Handle Ping/Pong for connection check
  if (msgType === MessageTypes.PING) {
    sendResponse({ type: MessageTypes.PONG, payload: { timestamp: message.payload?.timestamp } });

    // Also check native connection and report status
    const port = ensureNativeConnection();
    broadcastToSidebar({
      type: MessageTypes.CONNECTION_STATUS,
      payload: { connected: !!port }
    });
    return;
  }

  // Route based on message type
  switch (msgType) {
    // =============================================
    // Chat Sidebar -> Native Agent
    // =============================================
    case MessageTypes.CHAT_MESSAGE:
    case MessageTypes.STOP_GENERATION:
    case MessageTypes.UI_ACTION:
      forwardToNativeAgent(message);
      break;

    case MessageTypes.REQUEST_TAB_CONTEXT:
      getActiveTabContext().then((context) => {
        broadcastToSidebar({
          type: MessageTypes.TAB_CONTEXT_UPDATE,
          payload: context
        });
      });
      break;

    // =============================================
    // Chat Sidebar -> Content Sidebar (downstream)
    // =============================================
    case MessageTypes.DISPLAY_CONTENT:
    case MessageTypes.CLEAR_CONTENT:
    case MessageTypes.HIGHLIGHT_ELEMENT:
    case MessageTypes.CLEAR_HIGHLIGHT:
      // Forward to active tab's Content Sidebar
      getActiveTabId().then((tabId) => {
        if (tabId) {
          sendToContentSidebar(tabId, message);
        }
      });
      break;

    case MessageTypes.INJECT_CONTENT_SIDEBAR:
      injectContentSidebar(message.payload?.tab_id || null);
      break;

    // =============================================
    // Content Sidebar -> Chat Sidebar (upstream)
    // =============================================
    case MessageTypes.CONTENT_URL_REPORT:
    case MessageTypes.CONTENT_ELEMENT_CLICK:
      broadcastToSidebar(message);
      break;

    case MessageTypes.CONTENT_SIDEBAR_READY:
      // Track that Content Sidebar is ready in this tab
      const tabId = message.payload?.tab_id || sender.tab?.id;
      if (tabId) {
        contentSidebarStatus.set(tabId, true);
        console.log(`[NevoFlux] Content Sidebar ready in tab ${tabId}`);

        // Notify Chat Sidebar
        broadcastToSidebar({
          type: MessageTypes.CONTENT_SIDEBAR_INJECTED,
          payload: { tab_id: tabId, success: true }
        });
      }
      break;

    // =============================================
    // Page Context (Computer Use)
    // =============================================
    case MessageTypes.REQUEST_PAGE_CONTEXT:
      // Forward to Content Sidebar to extract page context
      handleRequestPageContext(message.payload?.session_id);
      break;

    case MessageTypes.PAGE_CONTEXT_RESPONSE:
      // Forward page context to Chat Sidebar (came from Content Sidebar)
      broadcastToSidebar(message);
      break;

    // =============================================
    // Tool Execution (Computer Use)
    // =============================================
    case MessageTypes.TOOL_CALL:
      // Route tool call to appropriate executor
      handleToolCall(message.payload);
      break;

    case MessageTypes.TOOL_RESULT:
      // Forward tool result to Native Agent (came from Content Sidebar)
      forwardToNativeAgent(message);
      break;

    case MessageTypes.AGENT_STATE_UPDATE:
      // Forward agent state to Chat Sidebar (came from Native Agent)
      broadcastToSidebar(message);
      break;

    // =============================================
    // Legacy message types (backwards compatibility)
    // =============================================
    case "agent_request":
      forwardToNativeAgent(message.data);
      break;

    case "get_page_content":
      getActiveTabContent().then(sendResponse).catch((e) => sendResponse({ error: e.message }));
      return true;

    case "browser_action":
      handleBrowserAction(message.data).then(sendResponse).catch((e) => sendResponse({ error: e.message }));
      return true;

    default:
      console.warn("[NevoFlux] Unknown message type:", msgType);
  }
});

/**
 * Forward message to native agent
 */
function forwardToNativeAgent(message) {
  const port = ensureNativeConnection();
  if (port) {
    port.postMessage(message);
  } else {
    console.error("[NevoFlux] Cannot forward - native port not connected");
    broadcastToSidebar({
      type: MessageTypes.AGENT_ERROR,
      payload: {
        session_id: message.payload?.session_id || "",
        code: "CONNECTION_ERROR",
        message: "Native messaging not available",
        recoverable: true
      }
    });
  }
}

/**
 * Get active tab ID
 */
async function getActiveTabId() {
  const tabs = await browser.tabs.query({ active: true, currentWindow: true });
  return tabs[0]?.id || null;
}

/**
 * Get active tab context
 */
async function getActiveTabContext() {
  const tabs = await browser.tabs.query({ active: true, currentWindow: true });
  const tab = tabs[0];

  if (!tab) {
    return {
      tab_id: 0,
      url: "",
      title: "",
      favicon_url: null,
      status: "complete"
    };
  }

  return {
    tab_id: tab.id,
    url: tab.url || "",
    title: tab.title || "",
    favicon_url: tab.favIconUrl || null,
    status: tab.status || "complete"
  };
}

/**
 * Get content from active tab via content script
 */
async function getActiveTabContent() {
  const tabs = await browser.tabs.query({ active: true, currentWindow: true });
  if (!tabs[0]) {
    throw new Error("No active tab");
  }

  return await browser.tabs.sendMessage(tabs[0].id, { type: "extract_content" });
}

/**
 * Inject Content Sidebar into a tab
 */
async function injectContentSidebar(targetTabId) {
  let tabId = targetTabId;

  if (!tabId) {
    tabId = await getActiveTabId();
  }

  if (!tabId) {
    console.error("[NevoFlux] No tab to inject Content Sidebar");
    return;
  }

  // Check if already injected
  if (contentSidebarStatus.get(tabId)) {
    console.log(`[NevoFlux] Content Sidebar already in tab ${tabId}`);
    return;
  }

  try {
    // Send message to bootstrap script (runs at document_start on all pages)
    // The bootstrap script will initialize the Content Sidebar
    await browser.tabs.sendMessage(tabId, {
      type: "inject_content_sidebar",
      target: "content-bootstrap"
    });

    console.log(`[NevoFlux] Sent inject request to tab ${tabId}`);
  } catch (error) {
    // If message fails, the bootstrap script might not be loaded yet
    // Fall back to scripting.executeScript
    console.warn(`[NevoFlux] Bootstrap message failed, using executeScript:`, error.message);

    try {
      await browser.scripting.executeScript({
        target: { tabId },
        files: ["content/content-bootstrap.js"]
      });
      console.log(`[NevoFlux] Injected bootstrap script into tab ${tabId}`);
    } catch (execError) {
      console.error(`[NevoFlux] Failed to inject Content Sidebar:`, execError);

      broadcastToSidebar({
        type: MessageTypes.CONTENT_SIDEBAR_INJECTED,
        payload: { tab_id: tabId, success: false }
      });
    }
  }
}

/**
 * Handle browser control commands from native agent
 */
async function handleBrowserControlFromAgent(control) {
  const { tab_id, action, selector, value } = control;

  switch (action) {
    case "navigate":
      await browser.tabs.update(tab_id, { url: value });
      break;

    case "click":
      await browser.tabs.sendMessage(tab_id, {
        type: "click_element",
        selector
      });
      break;

    case "highlight":
      await sendToContentSidebar(tab_id, {
        type: MessageTypes.HIGHLIGHT_ELEMENT,
        payload: { session_id: "", selector, style: "outline" }
      });
      break;

    case "scroll":
      await browser.tabs.sendMessage(tab_id, {
        type: "scroll_to",
        value
      });
      break;

    default:
      console.warn(`[NevoFlux] Unknown browser control action: ${action}`);
  }
}

/**
 * Handle browser automation actions (legacy)
 */
async function handleBrowserAction(action) {
  const { command, params } = action;

  switch (command) {
    case "navigate":
      const tab = await browser.tabs.create({ url: params.url });
      return { success: true, tabId: tab.id };

    case "click":
      const tabs = await browser.tabs.query({ active: true, currentWindow: true });
      if (tabs[0]) {
        return await browser.tabs.sendMessage(tabs[0].id, {
          type: "click_element",
          selector: params.selector
        });
      }
      throw new Error("No active tab");

    case "fill_form":
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

// =========================================================================
// Computer Use: Page Context
// =========================================================================

/**
 * Request page context from Content Sidebar
 */
async function handleRequestPageContext(sessionId) {
  const tabId = await getActiveTabId();
  if (!tabId) {
    console.warn("[NevoFlux] No active tab for page context request");
    return;
  }

  // Check if Content Sidebar is ready in this tab
  if (!contentSidebarStatus.get(tabId)) {
    console.warn(`[NevoFlux] Content Sidebar not ready in tab ${tabId}`);
    // Send empty context
    broadcastToSidebar({
      type: MessageTypes.PAGE_CONTEXT_RESPONSE,
      payload: {
        session_id: sessionId || "",
        tab_id: tabId,
        context: {
          url: "",
          title: "",
          viewport: { width: 0, height: 0, scroll_x: 0, scroll_y: 0, scroll_height: 0 },
          interactive_elements: [],
          text_content: null
        }
      }
    });
    return;
  }

  // Request context from Content Sidebar
  try {
    await browser.tabs.sendMessage(tabId, {
      target: "content-sidebar",
      type: MessageTypes.REQUEST_PAGE_CONTEXT,
      payload: { session_id: sessionId || "" }
    });
  } catch (error) {
    console.error("[NevoFlux] Failed to request page context:", error);
  }
}

// =========================================================================
// Computer Use: Tool Execution
// =========================================================================

/**
 * Route tool call to appropriate executor
 */
async function handleToolCall(payload) {
  if (!payload || !payload.tool_name) {
    console.error("[NevoFlux] Invalid tool call payload:", payload);
    return;
  }

  const { call_id, session_id, tool_name, parameters, show_feedback } = payload;
  console.log(`[NevoFlux] Tool call: ${tool_name}`, parameters);

  // Track pending call
  pendingToolCalls.set(call_id, { tool_name, session_id, timestamp: Date.now() });

  // Check if this tool should be executed by Background Script
  if (BACKGROUND_TOOLS.includes(tool_name)) {
    await executeBackgroundTool(call_id, session_id, tool_name, parameters);
  } else {
    // Forward to Content Sidebar for DOM operations
    const tabId = await getActiveTabId();
    if (!tabId) {
      sendToolResult(call_id, session_id, false, null, "No active tab");
      return;
    }

    if (!contentSidebarStatus.get(tabId)) {
      sendToolResult(call_id, session_id, false, null, "Content Sidebar not ready");
      return;
    }

    try {
      await browser.tabs.sendMessage(tabId, {
        target: "content-sidebar",
        type: MessageTypes.TOOL_CALL,
        payload: { call_id, session_id, tool_name, parameters, show_feedback }
      });
    } catch (error) {
      console.error(`[NevoFlux] Failed to send tool call to Content Sidebar:`, error);
      sendToolResult(call_id, session_id, false, null, error.message);
    }
  }
}

/**
 * Execute tools that run in Background Script (not Content Sidebar)
 */
async function executeBackgroundTool(callId, sessionId, toolName, parameters) {
  try {
    let result;

    switch (toolName) {
      case "screenshot":
        result = await captureScreenshot(parameters);
        break;

      case "navigate":
        result = await navigateToUrl(parameters);
        break;

      default:
        throw new Error(`Unknown background tool: ${toolName}`);
    }

    sendToolResult(callId, sessionId, true, result, null);

  } catch (error) {
    console.error(`[NevoFlux] Background tool ${toolName} failed:`, error);
    sendToolResult(callId, sessionId, false, null, error.message);
  }
}

/**
 * Capture screenshot of current tab
 */
async function captureScreenshot(params) {
  const tabs = await browser.tabs.query({ active: true, currentWindow: true });
  if (!tabs[0]) {
    throw new Error("No active tab");
  }

  const options = {
    format: "jpeg",
    quality: params?.quality || 80
  };

  // Note: captureVisibleTab may require additional permissions
  try {
    const dataUrl = await browser.tabs.captureVisibleTab(null, options);
    return {
      screenshot: dataUrl,
      tab_id: tabs[0].id,
      url: tabs[0].url,
      title: tabs[0].title
    };
  } catch (error) {
    // Fallback: may need activeTab permission
    throw new Error(`Screenshot failed: ${error.message}`);
  }
}

/**
 * Navigate to URL in active tab
 */
async function navigateToUrl(params) {
  const url = params?.url;
  if (!url) {
    throw new Error("URL is required for navigate tool");
  }

  const tabs = await browser.tabs.query({ active: true, currentWindow: true });
  if (!tabs[0]) {
    throw new Error("No active tab");
  }

  await browser.tabs.update(tabs[0].id, { url });

  return {
    success: true,
    tab_id: tabs[0].id,
    url: url
  };
}

/**
 * Send tool execution result back to Native Agent
 */
function sendToolResult(callId, sessionId, success, result, error) {
  // Remove from pending
  pendingToolCalls.delete(callId);

  const message = {
    type: MessageTypes.TOOL_RESULT,
    payload: {
      call_id: callId,
      session_id: sessionId,
      success,
      result: result || null,
      error: error || null,
      screenshot: null
    }
  };

  // Forward to Native Agent
  forwardToNativeAgent(message);
}

// Clean up pending tool calls periodically (timeout after 60 seconds)
setInterval(() => {
  const now = Date.now();
  for (const [callId, info] of pendingToolCalls) {
    if (now - info.timestamp > 60000) {
      console.warn(`[NevoFlux] Tool call ${callId} timed out`);
      sendToolResult(callId, info.session_id, false, null, "Tool execution timed out");
    }
  }
}, 10000);

// =========================================================================
// Event Listeners
// =========================================================================

// Clean up Content Sidebar status when tabs are closed
browser.tabs.onRemoved.addListener((tabId) => {
  contentSidebarStatus.delete(tabId);
});

// Update tab context when active tab changes
browser.tabs.onActivated.addListener(async (activeInfo) => {
  const context = await getActiveTabContext();
  broadcastToSidebar({
    type: MessageTypes.TAB_CONTEXT_UPDATE,
    payload: context
  });
});

// Update tab context when tab URL changes
browser.tabs.onUpdated.addListener(async (tabId, changeInfo, tab) => {
  if (changeInfo.status === "complete" || changeInfo.url) {
    const tabs = await browser.tabs.query({ active: true, currentWindow: true });
    if (tabs[0]?.id === tabId) {
      const context = await getActiveTabContext();
      broadcastToSidebar({
        type: MessageTypes.TAB_CONTEXT_UPDATE,
        payload: context
      });
    }
  }
});

// Handle keyboard commands from manifest.json
browser.commands.onCommand.addListener((command) => {
  console.log("[NevoFlux] Command received:", command);

  switch (command) {
    case "inject_content_sidebar":
      console.log("[NevoFlux] Injecting Content Sidebar via keyboard shortcut");
      injectContentSidebar(null);
      break;
  }
});

console.log("[NevoFlux] Background script loaded (Protocol v3.0)");
