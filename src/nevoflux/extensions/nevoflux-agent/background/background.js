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
 *
 * Protocol Version: 5.0 (2-channel architecture)
 *
 * Channels:
 * - Chat (Bidirectional): All Sidebar <-> Agent messages
 * - MCP (Bidirectional): Browser Use MCP requests/responses
 *
 * API Namespace: "bg:" prefix for all Sidebar-callable APIs
 */

// =============================================================================
// Channel Names (Native Messaging Application IDs)
// =============================================================================

const CHANNEL_NAMES = {
  CHAT: "com.nevoflux.agent",      // Chat channel (bidirectional)
  MCP: "com.nevoflux.agent.mcp",   // MCP channel (bidirectional)
};

// =============================================================================
// Background API (Sidebar callable, "bg:" prefix)
// =============================================================================

const BackgroundAPI = {
  // Channel management
  CONNECT: "bg:connect",
  DISCONNECT: "bg:disconnect",
  GET_STATUS: "bg:get_status",

  // MCP channel management
  MCP_ENABLE: "bg:mcp_enable",
  MCP_DISABLE: "bg:mcp_disable",

  // Send message to Native Agent
  SEND_TO_AGENT: "bg:send_to_agent",

  // Browser tool execution
  EXEC_TOOL: "bg:exec_tool",

  // Tab context
  GET_TAB_CONTEXT: "bg:get_tab_context",
};

// =============================================================================
// Message Type Constants
// =============================================================================

const MessageTypes = {
  // Sidebar -> Agent
  CHAT_MESSAGE: "chat_message",
  SKILL_COMMAND: "skill_command",
  STOP_GENERATION: "stop_generation",
  PERMISSION_RESPONSE: "permission_response",
  PLUGIN_COMMAND: "plugin_command",
  SYSTEM_COMMAND: "system_command",
  BROWSER_TOOL_RESPONSE: "browser_tool_response",

  // Agent -> Sidebar
  STREAM_CHUNK: "stream_chunk",
  STREAM_END: "stream_end",
  CONTENT_BLOCK: "content_block",
  PERMISSION_REQUEST: "permission_request",
  AGENT_STATE: "agent_state",
  ERROR: "error",
  ACCOUNT_STATUS: "account_status",
  SYSTEM_RESPONSE: "system_response",
  BROWSER_TOOL_REQUEST: "browser_tool_request",

  // MCP Channel
  MCP_REQUEST: "mcp_request",
  MCP_RESPONSE: "mcp_response",

  // System messages
  PING: "ping",
  PONG: "pong",
  CONNECTION_STATUS: "connection_status",

  // Legacy types (backwards compatibility)
  TAB_CONTEXT_UPDATE: "tab_context_update",
  REQUEST_TAB_CONTEXT: "request_tab_context",
};

// =============================================================================
// Reconnection Configuration
// =============================================================================

const RECONNECT_CONFIG = {
  initialDelay: 1000, // 1 second
  maxDelay: 30000, // 30 seconds
  multiplier: 2,
};

const MAX_RECONNECT_ATTEMPTS = 20;

// =============================================================================
// Chunking Configuration
// =============================================================================

/**
 * Configuration for message chunking to handle Firefox's 1MB native messaging limit
 */
const CHUNK_CONFIG = {
  maxMessageSize: 900_000, // 900KB threshold (leave 100KB buffer)
  chunkSize: 800_000, // 800KB per chunk
  timeout: 30_000, // 30 seconds reassembly timeout
};

/**
 * Generate a unique chunk ID
 */
function generateChunkId() {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Check if a message needs to be chunked
 * @param {Object} message - Message to check
 * @returns {boolean} - Whether chunking is needed
 */
function needsChunking(message) {
  const json = JSON.stringify(message);
  return json.length > CHUNK_CONFIG.maxMessageSize;
}

/**
 * Split a large message into chunks
 * @param {Object} message - Original message to split
 * @returns {Array<Object>} - Array of chunk envelopes
 */
function chunkMessage(message) {
  const json = JSON.stringify(message);
  const base64Data = btoa(unescape(encodeURIComponent(json)));
  const chunks = [];
  const chunkId = generateChunkId();
  const totalChunks = Math.ceil(base64Data.length / CHUNK_CONFIG.chunkSize);

  for (let i = 0; i < totalChunks; i++) {
    const start = i * CHUNK_CONFIG.chunkSize;
    const end = Math.min(start + CHUNK_CONFIG.chunkSize, base64Data.length);
    const chunkData = base64Data.slice(start, end);

    chunks.push({
      __chunk: {
        id: chunkId,
        index: i,
        total: totalChunks,
        data: chunkData,
      },
    });
  }

  console.log(`[NevoFlux] Chunked message into ${totalChunks} chunks (original: ${json.length} bytes)`);
  return chunks;
}

/**
 * Class to reassemble chunked messages
 */
class ChunkReassembler {
  constructor() {
    // Map of chunk ID -> { chunks: Map<index, data>, total: number, timestamp: number }
    this.pending = new Map();
  }

  /**
   * Check if a message is a chunk envelope
   * @param {Object} message - Message to check
   * @returns {boolean}
   */
  isChunk(message) {
    return message && typeof message.__chunk === "object" && message.__chunk !== null;
  }

  /**
   * Process a chunk and return the complete message if all chunks are received
   * @param {Object} chunkEnvelope - Chunk envelope message
   * @returns {Object|null} - Complete message or null if still waiting for chunks
   */
  processChunk(chunkEnvelope) {
    const { id, index, total, data } = chunkEnvelope.__chunk;

    console.log(`[NevoFlux] Processing chunk ${index + 1}/${total} for message ${id}`);

    // Get or create pending entry
    let pending = this.pending.get(id);
    if (!pending) {
      pending = {
        chunks: new Map(),
        total,
        timestamp: Date.now(),
      };
      this.pending.set(id, pending);
    }

    // Store chunk data
    pending.chunks.set(index, data);

    // Check if all chunks received
    if (pending.chunks.size === total) {
      // Reassemble in order
      let fullBase64 = "";
      for (let i = 0; i < total; i++) {
        fullBase64 += pending.chunks.get(i);
      }

      // Decode base64 to JSON
      try {
        const json = decodeURIComponent(escape(atob(fullBase64)));
        const message = JSON.parse(json);

        console.log(`[NevoFlux] Reassembled message from ${total} chunks`);

        // Cleanup
        this.pending.delete(id);
        this.cleanupOldPending();

        return message;
      } catch (e) {
        console.error(`[NevoFlux] Failed to reassemble message ${id}:`, e);
        this.pending.delete(id);
        return null;
      }
    }

    return null;
  }

  /**
   * Cleanup pending chunks that have timed out
   */
  cleanupOldPending() {
    const now = Date.now();
    for (const [id, pending] of this.pending) {
      if (now - pending.timestamp > CHUNK_CONFIG.timeout) {
        console.warn(`[NevoFlux] Chunk reassembly timed out for message ${id}`);
        this.pending.delete(id);
      }
    }
  }
}

// Global chunk reassembler instance
const chunkReassembler = new ChunkReassembler();

// =============================================================================
// Native Channel Class
// =============================================================================

/**
 * Manages a single native messaging channel with reconnection support
 */
class NativeChannel {
  constructor(name, displayName, onMessage, onStatusChange) {
    this.name = name;
    this.displayName = displayName;
    this.onMessage = onMessage;
    this.onStatusChange = onStatusChange;
    this.port = null;
    this.listeners = null;
    this.reconnectAttempts = 0;
    this.reconnectTimer = null;
    this.isIntentionalDisconnect = false;
    this.connectionInProgress = false;
  }

  /**
   * Connect to the native messaging host
   */
  connect() {
    if (this.connectionInProgress) {
      console.log(`[NevoFlux] ${this.displayName} channel connection already in progress`);
      return false;
    }

    if (this.port) {
      console.log(`[NevoFlux] ${this.displayName} channel already connected`);
      return true;
    }

    this.connectionInProgress = true;
    console.log(`[NevoFlux] Connecting ${this.displayName} channel (${this.name})...`);

    try {
      this.port = browser.runtime.connectNative(this.name);

      this.listeners = {
        onMessage: (message) => {
          // Check if this is a chunk that needs reassembly
          if (chunkReassembler.isChunk(message)) {
            const reassembled = chunkReassembler.processChunk(message);
            if (reassembled) {
              console.log(`[NevoFlux] ${this.displayName} received (reassembled):`, reassembled.type || reassembled);
              if (this.onMessage) {
                this.onMessage(reassembled);
              }
            }
            // If not fully reassembled yet, wait for more chunks
            return;
          }

          console.log(`[NevoFlux] ${this.displayName} received:`, message);
          if (this.onMessage) {
            this.onMessage(message);
          }
        },
        onDisconnect: (port) => {
          const errorMsg = port.error ? port.error.message || String(port.error) : "null";
          console.error(`[NevoFlux] ${this.displayName} channel disconnected:`, errorMsg);
          this.cleanup();

          if (this.onStatusChange) {
            this.onStatusChange(false, errorMsg);
          }

          // Auto-reconnect if not intentional
          if (!this.isIntentionalDisconnect) {
            this.scheduleReconnect();
          }
        },
      };

      this.port.onMessage.addListener(this.listeners.onMessage);
      this.port.onDisconnect.addListener(this.listeners.onDisconnect);

      // Reset reconnect state on successful connection
      this.reconnectAttempts = 0;
      this.isIntentionalDisconnect = false;
      this.connectionInProgress = false;

      console.log(`[NevoFlux] ${this.displayName} channel connected`);

      if (this.onStatusChange) {
        this.onStatusChange(true, null);
      }

      return true;
    } catch (error) {
      console.error(`[NevoFlux] Failed to connect ${this.displayName} channel:`, error);
      this.connectionInProgress = false;
      this.cleanup();

      if (this.onStatusChange) {
        this.onStatusChange(false, error.message);
      }

      return false;
    }
  }

  /**
   * Disconnect from the native messaging host
   */
  disconnect() {
    this.isIntentionalDisconnect = true;
    this.cancelReconnect();
    this.cleanup();
    console.log(`[NevoFlux] ${this.displayName} channel disconnected intentionally`);
  }

  /**
   * Cleanup port and listeners
   */
  cleanup() {
    if (this.port && this.listeners) {
      try {
        this.port.onMessage.removeListener(this.listeners.onMessage);
        this.port.onDisconnect.removeListener(this.listeners.onDisconnect);
      } catch (e) {
        // Ignore errors during listener removal
      }
      try {
        this.port.disconnect();
      } catch (e) {
        // Ignore errors during disconnect
      }
    }
    this.port = null;
    this.listeners = null;
  }

  /**
   * Schedule a reconnection attempt with exponential backoff
   */
  scheduleReconnect() {
    if (this.reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
      console.error(`[NevoFlux] Max reconnection attempts (${MAX_RECONNECT_ATTEMPTS}) reached for ${this.displayName}`);
      this.onStatusChange?.(false);
      return;
    }

    this.cancelReconnect();

    // Increment before scheduling to avoid race condition with concurrent reconnect attempts
    this.reconnectAttempts++;

    const delay = Math.min(
      RECONNECT_CONFIG.initialDelay * Math.pow(RECONNECT_CONFIG.multiplier, this.reconnectAttempts - 1),
      RECONNECT_CONFIG.maxDelay
    );

    console.log(`[NevoFlux] Scheduling ${this.displayName} reconnect in ${delay}ms (attempt ${this.reconnectAttempts})`);

    this.reconnectTimer = setTimeout(() => {
      if (!this.connect()) {
        // If reconnect fails, schedule another attempt
        this.scheduleReconnect();
      }
    }, delay);
  }

  /**
   * Cancel any pending reconnection attempt
   */
  cancelReconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  /**
   * Send a message through the channel
   * Automatically chunks large messages to handle Firefox's 1MB native messaging limit
   * @param {Object} message - Message to send
   * @returns {boolean} - Whether the message was sent
   */
  send(message) {
    if (!this.port) {
      console.warn(`[NevoFlux] Cannot send to ${this.displayName} - not connected`);
      return false;
    }

    try {
      // Check if message needs chunking
      if (needsChunking(message)) {
        const chunks = chunkMessage(message);
        for (const chunk of chunks) {
          this.port.postMessage(chunk);
        }
        console.log(`[NevoFlux] ${this.displayName} sent ${chunks.length} chunks for:`, message.type || message);
        return true;
      }

      // Send directly for small messages
      this.port.postMessage(message);
      console.log(`[NevoFlux] ${this.displayName} sent:`, message.type || message);
      return true;
    } catch (error) {
      console.error(`[NevoFlux] Failed to send to ${this.displayName}:`, error);
      return false;
    }
  }

  /**
   * Check if channel is connected
   */
  isConnected() {
    return this.port !== null;
  }
}

// =============================================================================
// Channel Manager (Simplified for 2-channel architecture)
// =============================================================================

/**
 * Manages Chat and MCP native messaging channels
 */
class ChannelManager {
  constructor() {
    // Chat channel: Sidebar <-> Agent (bidirectional)
    this.chat = new NativeChannel(
      CHANNEL_NAMES.CHAT,
      "Chat",
      (msg) => this.handleChatMessage(msg),
      (connected, error) => this.handleChatStatusChange(connected, error)
    );

    // MCP channel: Browser Use MCP (bidirectional)
    this.mcp = new NativeChannel(
      CHANNEL_NAMES.MCP,
      "MCP",
      (msg) => this.handleMcpMessage(msg),
      (connected, error) => this.handleMcpStatusChange(connected, error)
    );

    this.connectionStatus = { chat: false, mcp: false };
    this.mcpEnabled = false;
  }

  /**
   * Connect the Chat channel
   */
  connect() {
    console.log("[NevoFlux] Connecting Chat channel...");
    this.chat.connect();
  }

  /**
   * Disconnect all channels
   */
  disconnect() {
    this.chat.disconnect();
    if (this.mcpEnabled) {
      this.mcp.disconnect();
    }
  }

  /**
   * Enable/disable MCP channel
   */
  setMcpEnabled(enabled) {
    this.mcpEnabled = enabled;
    if (enabled && !this.mcp.isConnected()) {
      this.mcp.connect();
    } else if (!enabled && this.mcp.isConnected()) {
      this.mcp.disconnect();
    }
  }

  /**
   * Send message to Native Agent via Chat channel
   */
  sendToAgent(message) {
    if (!this.chat.isConnected()) {
      console.warn("[NevoFlux] Chat channel not connected, attempting to connect...");
      this.chat.connect();
    }
    return this.chat.send(message);
  }

  /**
   * Send message to MCP channel
   */
  sendToMcp(message) {
    if (!this.mcp.isConnected()) {
      if (!this.mcpEnabled) {
        console.warn("[NevoFlux] MCP channel is disabled");
        return false;
      }
      this.mcp.connect();
    }
    return this.mcp.send(message);
  }

  /**
   * Handle messages from Chat channel
   * All messages are broadcast to Sidebar - Sidebar decides how to handle
   */
  handleChatMessage(message) {
    console.log("[NevoFlux] Chat channel received:", message.type);

    // All messages go to Sidebar - Sidebar decides how to handle
    broadcastToSidebar(message);
  }

  /**
   * Handle messages from MCP channel
   */
  handleMcpMessage(message) {
    const msgType = message.type;
    if (msgType === MessageTypes.MCP_REQUEST) {
      handleMcpRequest(message.payload);
    } else if (msgType === MessageTypes.MCP_RESPONSE) {
      broadcastToSidebar(message);
    } else {
      console.warn(`[NevoFlux] Unknown MCP message type: ${msgType}`);
    }
  }

  /**
   * Handle Chat channel status change
   */
  handleChatStatusChange(connected, error) {
    this.connectionStatus.chat = connected;
    if (connected) {
      console.log("[NevoFlux] Chat channel connected (bidirectional communication ready)");
    }
    this.broadcastConnectionStatus();
  }

  /**
   * Handle MCP channel status change
   */
  handleMcpStatusChange(connected, error) {
    this.connectionStatus.mcp = connected;
    this.broadcastConnectionStatus();
  }

  /**
   * Broadcast connection status to Sidebar
   */
  broadcastConnectionStatus() {
    broadcastToSidebar({
      type: MessageTypes.CONNECTION_STATUS,
      payload: {
        connected: this.connectionStatus.chat,
        channels: { ...this.connectionStatus },
      },
    });
  }

  /**
   * Get current connection status
   */
  getStatus() {
    return {
      connected: this.connectionStatus.chat,
      channels: { ...this.connectionStatus },
    };
  }
}

// =============================================================================
// Global Channel Manager Instance
// =============================================================================

const channelManager = new ChannelManager();

// =============================================================================
// Sidebar Communication
// =============================================================================

/**
 * Broadcast message to sidebar (Chat Sidebar Dioxus app)
 */
function broadcastToSidebar(message) {
  browser.runtime.sendMessage(message).catch((err) => {
    // Sidebar might not be open - this is normal
    console.debug("[NevoFlux] Sidebar not available:", err.message);
  });
}

// =============================================================================
// MCP Request Handler
// =============================================================================

/**
 * Handle MCP request from external agent
 * Routes to Browser Use API via JSWindowActor
 */
async function handleMcpRequest(payload) {
  const { request_id, source, payload: jsonRpcRequest } = payload;
  console.log(`[NevoFlux] MCP request from ${source?.agent || "unknown"}:`, jsonRpcRequest.method);

  try {
    // Get active tab to execute Browser Use API
    const tabs = await browser.tabs.query({ active: true, currentWindow: true });
    if (!tabs[0]) {
      throw new Error("No active tab");
    }

    // Send to content script which will communicate with JSWindowActor
    const result = await browser.tabs.sendMessage(tabs[0].id, {
      target: "browser-use-api",
      type: "mcp_execute",
      payload: jsonRpcRequest,
    });

    // Send response back through MCP channel
    channelManager.sendToMcp({
      type: MessageTypes.MCP_RESPONSE,
      payload: {
        request_id,
        payload: {
          jsonrpc: "2.0",
          id: jsonRpcRequest.id,
          result,
        },
      },
    });
  } catch (error) {
    console.error("[NevoFlux] MCP request failed:", error);

    // Send error response
    channelManager.sendToMcp({
      type: MessageTypes.MCP_RESPONSE,
      payload: {
        request_id,
        payload: {
          jsonrpc: "2.0",
          id: jsonRpcRequest.id,
          error: {
            code: -32603,
            message: error.message,
          },
        },
      },
    });
  }
}

// =============================================================================
// Tab Context Helpers
// =============================================================================

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
      status: "complete",
    };
  }

  return {
    tab_id: tab.id,
    url: tab.url || "",
    title: tab.title || "",
    favicon_url: tab.favIconUrl || null,
    status: tab.status || "complete",
  };
}

// =============================================================================
// Browser Tool Execution (via browser.nevoflux.* API)
// =============================================================================

/**
 * Execute a browser tool action using browser.nevoflux.* API
 * This uses Firefox's privileged windowUtils for trusted events (isTrusted=true)
 *
 * @param {object} request - Browser tool request payload
 * @param {string} caller - Caller identifier ("sidebar" or "mcp")
 * @returns {Promise<{success: boolean, result?: any, error?: object}>}
 */
async function executeBrowserTool(request, caller = "unknown") {
  const { action, params, tab_id, timeout_ms = 30000 } = request;

  // Get target tab
  let targetTabId = tab_id;
  if (!targetTabId) {
    targetTabId = await getActiveTabId();
    if (!targetTabId) {
      return { success: false, error: { code: -1, message: "No active tab", recoverable: true } };
    }
  }

  // Check if browser.nevoflux API is available
  const useNevofluxApi = isNevofluxApiAvailable();
  console.log(`[NevoFlux] [${caller}] Executing browser tool: ${action} on tab ${targetTabId} (nevoflux API: ${useNevofluxApi})`);

  try {
    switch (action) {
      // Navigation
      case "navigate":
        return await executeNavigateViaApi(targetTabId, params);

      // Selector-based interactions (uses trusted events via windowUtils)
      case "click":
        return await executeClickViaApi(targetTabId, params);

      case "type":
        return await executeTypeViaApi(targetTabId, params);

      case "fill":
        return await executeFillViaApi(targetTabId, params);

      // Data extraction
      case "get_content":
        return await executeGetContentViaApi(targetTabId, params);

      case "screenshot":
        return await executeScreenshotViaApi(targetTabId, params);

      // JavaScript execution
      case "eval_js":
        return await executeEvalJsViaApi(targetTabId, params);

      // Waiting
      case "wait_for":
        return await executeWaitForViaApi(targetTabId, params, timeout_ms);

      // Scrolling
      case "scroll":
        return await executeScrollViaApi(targetTabId, params);

      // Element queries
      case "get_element":
        return await executeGetElementViaApi(targetTabId, params);

      case "query_all":
        return await executeQueryAllViaApi(targetTabId, params);

      // Snapshot-based tools (element ID approach)
      case "snapshot":
        return await executeSnapshotViaApi(targetTabId, params);

      case "click_by_id":
        return await executeClickByIdViaApi(targetTabId, params, timeout_ms);

      case "fill_by_id":
        return await executeFillByIdViaApi(targetTabId, params, timeout_ms);

      case "type_by_id":
        return await executeTypeByIdViaApi(targetTabId, params, timeout_ms);

      // Keyboard control
      case "key_press":
        return await executeKeyPressViaApi(targetTabId, params);

      // Content extraction
      case "get_markdown":
        return await executeGetMarkdownViaApi(targetTabId, params);

      default:
        return { success: false, error: { code: -1, message: `Unknown action: ${action}`, recoverable: false } };
    }
  } catch (error) {
    console.error(`[NevoFlux] Browser tool error:`, error);
    return {
      success: false,
      error: { code: -1, message: error.message || String(error), recoverable: true },
    };
  }
}

// =============================================================================
// browser.nevoflux.* API Implementation Functions
// These use Firefox's privileged windowUtils for trusted events (isTrusted=true)
// =============================================================================

/**
 * Check if browser.nevoflux API is available
 * @returns {boolean}
 */
function isNevofluxApiAvailable() {
  const hasBrowser = typeof browser !== "undefined";
  const hasNevoflux = hasBrowser && typeof browser.nevoflux !== "undefined";
  const hasClick = hasNevoflux && typeof browser.nevoflux.click === "function";

  console.log(`[NevoFlux] API check: browser=${hasBrowser}, nevoflux=${hasNevoflux}, click=${hasClick}`);

  return hasClick;
}

/**
 * Navigate to a URL via browser.nevoflux.open()
 */
async function executeNavigateViaApi(tabId, params) {
  const { url } = params;
  if (!url) {
    return { success: false, error: { code: -1, message: "URL required", recoverable: false } };
  }

  try {
    const result = await browser.nevoflux.open(tabId, url);
    if (result.success === false) {
      return result;
    }

    // Wait for page load
    return new Promise((resolve) => {
      const listener = (updatedTabId, changeInfo) => {
        if (updatedTabId === tabId && changeInfo.status === "complete") {
          browser.tabs.onUpdated.removeListener(listener);
          resolve({ success: true, result: { url } });
        }
      };
      browser.tabs.onUpdated.addListener(listener);

      // Timeout after 30 seconds
      setTimeout(() => {
        browser.tabs.onUpdated.removeListener(listener);
        resolve({ success: true, result: { url, note: "Navigation started but completion not confirmed" } });
      }, 30000);
    });
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Click element via browser.nevoflux.click() - uses trusted mouse events
 * Falls back to content script if API is not available
 */
async function executeClickViaApi(tabId, params) {
  const { selector, button = "left", click_count = 1 } = params;
  if (!selector) {
    return { success: false, error: { code: -1, message: "selector required", recoverable: false } };
  }

  // Fallback to content script if API not available
  if (!isNevofluxApiAvailable()) {
    console.log("[NevoFlux] browser.nevoflux not available, using content script");
    return await executeInContentScript(tabId, "click", params, 30000);
  }

  try {
    const result = await browser.nevoflux.click(tabId, selector, {
      button,
      clickCount: click_count,
    });
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    console.error("[NevoFlux] nevoflux.click failed, falling back to content script:", error.message);
    return await executeInContentScript(tabId, "click", params, 30000);
  }
}

/**
 * Type text via browser.nevoflux.type() - uses trusted keyboard events
 * Falls back to content script if API is not available
 */
async function executeTypeViaApi(tabId, params) {
  const { selector, text } = params;
  if (!selector || text === undefined) {
    return { success: false, error: { code: -1, message: "selector and text required", recoverable: false } };
  }

  if (!isNevofluxApiAvailable()) {
    return await executeInContentScript(tabId, "type", params, 30000);
  }

  try {
    const result = await browser.nevoflux.type(tabId, selector, text);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    console.error("[NevoFlux] nevoflux.type failed, falling back to content script:", error.message);
    return await executeInContentScript(tabId, "type", params, 30000);
  }
}

/**
 * Fill input via browser.nevoflux.fill()
 * Falls back to content script if API is not available
 */
async function executeFillViaApi(tabId, params) {
  const { selector, value } = params;
  if (!selector || value === undefined) {
    return { success: false, error: { code: -1, message: "selector and value required", recoverable: false } };
  }

  if (!isNevofluxApiAvailable()) {
    return await executeInContentScript(tabId, "fill", params, 30000);
  }

  try {
    const result = await browser.nevoflux.fill(tabId, selector, value);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    console.error("[NevoFlux] nevoflux.fill failed, falling back to content script:", error.message);
    return await executeInContentScript(tabId, "fill", params, 30000);
  }
}

/**
 * Get content via browser.nevoflux.getText() or snapshot()
 */
async function executeGetContentViaApi(tabId, params) {
  const { selector } = params;

  try {
    if (selector) {
      const text = await browser.nevoflux.getText(tabId, selector);
      return { success: true, result: { selector, text } };
    } else {
      // Get full page snapshot
      const result = await browser.nevoflux.snapshot(tabId, params);
      return result.success !== undefined ? result : { success: true, result };
    }
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Take screenshot via browser.nevoflux.screenshot()
 */
async function executeScreenshotViaApi(tabId, params) {
  const { full_page = false } = params;

  try {
    const result = await browser.nevoflux.screenshot(tabId, { fullPage: full_page });
    if (result.success === false) {
      return result;
    }
    return {
      success: true,
      result: {
        data_url: `data:${result.mimeType};base64,${result.data}`,
        width: result.width,
        height: result.height,
        full_page,
      },
    };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Execute JavaScript via browser.nevoflux.eval()
 */
async function executeEvalJsViaApi(tabId, params) {
  const { script } = params;
  if (!script) {
    return { success: false, error: { code: -1, message: "Script required", recoverable: false } };
  }

  try {
    const result = await browser.nevoflux.eval(tabId, script);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Wait for selector via browser.nevoflux.waitForSelector()
 */
async function executeWaitForViaApi(tabId, params, timeout_ms) {
  const { selector, state = "visible" } = params;
  if (!selector) {
    return { success: false, error: { code: -1, message: "selector required", recoverable: false } };
  }

  try {
    const result = await browser.nevoflux.waitForSelector(tabId, selector, {
      timeout: timeout_ms,
      state,
    });
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Scroll via browser.nevoflux.wheel()
 */
async function executeScrollViaApi(tabId, params) {
  const { direction = "down", amount = 300 } = params;

  const deltaMap = {
    up: { deltaX: 0, deltaY: -amount },
    down: { deltaX: 0, deltaY: amount },
    left: { deltaX: -amount, deltaY: 0 },
    right: { deltaX: amount, deltaY: 0 },
  };

  const delta = deltaMap[direction] || deltaMap.down;

  try {
    const result = await browser.nevoflux.wheel(tabId, delta);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Get element info via browser.nevoflux.exists() and isVisible()
 */
async function executeGetElementViaApi(tabId, params) {
  const { selector } = params;
  if (!selector) {
    return { success: false, error: { code: -1, message: "selector required", recoverable: false } };
  }

  try {
    const exists = await browser.nevoflux.exists(tabId, selector);
    if (!exists) {
      return {
        success: false,
        error: { code: -1, message: `Element not found: ${selector}`, recoverable: true },
      };
    }

    const visible = await browser.nevoflux.isVisible(tabId, selector);
    return {
      success: true,
      result: { selector, exists: true, visible },
    };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Query all elements via browser.nevoflux.eval()
 */
async function executeQueryAllViaApi(tabId, params) {
  const { selector, limit = 50 } = params;
  if (!selector) {
    return { success: false, error: { code: -1, message: "selector required", recoverable: false } };
  }

  const script = `(function() {
    const elements = document.querySelectorAll('${selector.replace(/'/g, "\\'")}');
    const results = [];
    for (let i = 0; i < Math.min(elements.length, ${limit}); i++) {
      const el = elements[i];
      const rect = el.getBoundingClientRect();
      results.push({
        tag: el.tagName.toLowerCase(),
        id: el.id || null,
        text: el.textContent?.substring(0, 100) || '',
        visible: rect.width > 0 && rect.height > 0,
      });
    }
    return { count: results.length, elements: results };
  })()`;

  try {
    const result = await browser.nevoflux.eval(tabId, script);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

// Store element refs from snapshot for later use by click_by_id, fill_by_id, etc.
// Key: tabId, Value: { refs: {element_id -> {selector, role, name, tagName}}, timestamp }
const snapshotRefs = new Map();

// Cleanup old snapshots after 5 minutes
const SNAPSHOT_MAX_AGE_MS = 300000;

function cleanupOldSnapshots() {
  const now = Date.now();
  for (const [tabId, data] of snapshotRefs) {
    if (now - data.timestamp > SNAPSHOT_MAX_AGE_MS) {
      snapshotRefs.delete(tabId);
    }
  }
}

/**
 * Get page snapshot via browser.nevoflux.snapshot()
 */
async function executeSnapshotViaApi(tabId, params) {
  if (!isNevofluxApiAvailable()) {
    console.log("[NevoFlux] browser.nevoflux not available for snapshot");
    return { success: false, error: { code: -1, message: "browser.nevoflux API not available", recoverable: false } };
  }

  try {
    // Remove tab_id from params as it's passed separately
    const { tab_id, ...options } = params || {};
    console.log(`[NevoFlux] Calling browser.nevoflux.snapshot(${tabId}, ${JSON.stringify(options)})`);
    const result = await browser.nevoflux.snapshot(tabId, options);
    console.log(`[NevoFlux] Snapshot result:`, result);

    if (result.error) {
      return { success: false, error: result.error };
    }

    // Store refs for later use by click_by_id, fill_by_id, etc.
    // Convert refs keys from "e1", "e2" to numeric 1, 2 for compatibility
    const numericRefs = {};
    if (result.refs) {
      for (const [key, value] of Object.entries(result.refs)) {
        // Convert "e1" -> 1, "e2" -> 2, etc.
        const numericId = parseInt(key.replace(/^e/, ""), 10);
        if (!isNaN(numericId)) {
          numericRefs[numericId] = value;
        }
      }
    }

    snapshotRefs.set(tabId, {
      refs: numericRefs,
      timestamp: Date.now(),
    });

    // Cleanup old snapshots periodically
    cleanupOldSnapshots();

    console.log(`[NevoFlux] Stored ${Object.keys(numericRefs).length} element refs for tab ${tabId}`);
    // Debug: Log a few specific elements to verify storage
    if (numericRefs[31]) {
      console.log(`[NevoFlux] Element 31 stored:`, JSON.stringify(numericRefs[31]).substring(0, 300));
    }
    console.log(`[NevoFlux] First 5 elements:`, Object.entries(numericRefs).slice(0, 5).map(([k, v]) => `${k}: ${v.selector?.substring(0, 50)}`).join("; "));

    return {
      success: true,
      result: {
        tree: result.tree,
        refs: result.refs,
        element_count: Object.keys(result.refs || {}).length,
        stats: result.stats || { total: Object.keys(result.refs || {}).length, fromA11y: 0, fromDom: 0 },
        url: result.url || "",
        title: result.title || "",
      },
    };
  } catch (error) {
    console.error("[NevoFlux] browser.nevoflux.snapshot failed:", error.message);
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Get element selector from element ID (stored from snapshot result)
 */
async function getElementSelector(tabId, elementId) {
  // Look up from stored snapshot refs
  const tabData = snapshotRefs.get(tabId);

  console.log(`[NevoFlux] getElementSelector called: tabId=${tabId}, elementId=${elementId}`);
  console.log(`[NevoFlux] snapshotRefs has keys:`, Array.from(snapshotRefs.keys()));

  if (!tabData) {
    console.warn(`[NevoFlux] No snapshot data for tab ${tabId}. Take a snapshot first.`);
    return null;
  }

  console.log(`[NevoFlux] tabData.timestamp:`, tabData.timestamp, `(${(Date.now() - tabData.timestamp) / 1000}s ago)`);
  console.log(`[NevoFlux] tabData.refs has ${Object.keys(tabData.refs).length} elements`);
  console.log(`[NevoFlux] Available element IDs (first 20):`, Object.keys(tabData.refs).slice(0, 20).join(", "));

  const elementRef = tabData.refs[elementId];

  if (!elementRef) {
    console.warn(`[NevoFlux] Element ID ${elementId} not found in snapshot refs.`);
    // Log nearby IDs to help debug
    const allIds = Object.keys(tabData.refs).map(Number).sort((a, b) => a - b);
    const nearbyIds = allIds.filter(id => Math.abs(id - elementId) <= 5);
    console.warn(`[NevoFlux] Nearby IDs: ${nearbyIds.join(", ")}`);
    return null;
  }

  console.log(`[NevoFlux] Found element ${elementId}:`, JSON.stringify(elementRef).substring(0, 300));
  return elementRef.selector;
}

/**
 * Click element by ID via browser.nevoflux.click() - uses trusted mouse events
 * Falls back to content script if API is not available or fails
 */
async function executeClickByIdViaApi(tabId, params, timeout_ms) {
  const { element_id } = params;

  if (!element_id) {
    return { success: false, error: { code: -1, message: "element_id required", recoverable: false } };
  }

  // Fallback to content script if API not available
  if (!isNevofluxApiAvailable()) {
    console.log("[NevoFlux] browser.nevoflux not available, using content script for click_by_id");
    return await executeInContentScript(tabId, "click_by_id", params, timeout_ms);
  }

  try {
    // Get selector from element ID
    const selector = await getElementSelector(tabId, element_id);

    if (!selector) {
      return {
        success: false,
        error: { code: -1, message: `Element ID ${element_id} not found. Take a new snapshot first.`, recoverable: true },
      };
    }

    console.log(`[NevoFlux] Clicking element ${element_id} via browser.nevoflux.click('${selector}')`);

    // Use browser.nevoflux.click with trusted mouse events
    const result = await browser.nevoflux.click(tabId, selector);

    if (result.success === false) {
      return result;
    }

    return {
      success: true,
      result: { element_id, selector, clicked: true, method: "nevoflux_api" },
    };
  } catch (error) {
    console.error("[NevoFlux] nevoflux.click failed:", error.message);
    // Fallback to content script
    return await executeInContentScript(tabId, "click_by_id", params, timeout_ms);
  }
}

/**
 * Fill element by ID via browser.nevoflux.fill() + keyPress for Enter
 * Falls back to content script if API is not available or fails
 */
async function executeFillByIdViaApi(tabId, params, timeout_ms) {
  const { element_id, value, press_enter = false } = params;

  console.log(`[NevoFlux] executeFillByIdViaApi: element_id=${element_id}, value=${value?.substring(0, 20)}, press_enter=${press_enter}`);

  if (!element_id || value === undefined) {
    return { success: false, error: { code: -1, message: "element_id and value required", recoverable: false } };
  }

  // Fallback to content script if API not available
  if (!isNevofluxApiAvailable()) {
    console.log("[NevoFlux] browser.nevoflux not available, using content script for fill_by_id");
    return await executeInContentScript(tabId, "fill_by_id", params, timeout_ms);
  }

  try {
    // Get selector from element ID
    console.log(`[NevoFlux] Getting selector for element_id=${element_id}`);
    const selector = await getElementSelector(tabId, element_id);
    console.log(`[NevoFlux] Got selector: ${selector}`);

    if (!selector) {
      return {
        success: false,
        error: { code: -1, message: `Element ID ${element_id} not found. Take a new snapshot first.`, recoverable: true },
      };
    }

    console.log(`[NevoFlux] Filling element ${element_id} via browser.nevoflux.fill('${selector}', '${value.substring(0, 20)}...')`);

    // Click to focus first
    console.log(`[NevoFlux] Step 1: Clicking to focus...`);
    const clickResult = await browser.nevoflux.click(tabId, selector);
    console.log(`[NevoFlux] Click result:`, clickResult);
    await new Promise(r => setTimeout(r, 100));

    // Clear and fill
    console.log(`[NevoFlux] Step 2: Clearing...`);
    const clearResult = await browser.nevoflux.clear(tabId, selector);
    console.log(`[NevoFlux] Clear result:`, clearResult);

    console.log(`[NevoFlux] Step 3: Filling...`);
    const result = await browser.nevoflux.fill(tabId, selector, value);
    console.log(`[NevoFlux] Fill result:`, result);

    if (result.success === false) {
      return result;
    }

    // Press Enter if requested - uses trusted keyboard events
    if (press_enter) {
      await new Promise(r => setTimeout(r, 100));
      // Focus again in case it was lost
      console.log(`[NevoFlux] Step 4: Re-focusing...`);
      const focusResult = await browser.nevoflux.focus(tabId, selector);
      console.log(`[NevoFlux] Focus result:`, focusResult);
      await new Promise(r => setTimeout(r, 50));

      console.log(`[NevoFlux] Step 5: Pressing Enter...`);
      const enterResult = await browser.nevoflux.keyPress(tabId, "Enter");
      console.log(`[NevoFlux] Enter result:`, enterResult);
    }

    return {
      success: true,
      result: { element_id, selector, filled: value, enter_pressed: press_enter, method: "nevoflux_api" },
    };
  } catch (error) {
    console.error("[NevoFlux] nevoflux.fill failed at some step:", error.message, error.stack);
    // Fallback to content script
    return await executeInContentScript(tabId, "fill_by_id", params, timeout_ms);
  }
}

/**
 * Type text into element by ID via browser.nevoflux.type() - uses trusted keyboard events
 * Falls back to content script if API is not available or fails
 */
async function executeTypeByIdViaApi(tabId, params, timeout_ms) {
  const { element_id, text, press_enter = false } = params;

  if (!element_id || text === undefined) {
    return { success: false, error: { code: -1, message: "element_id and text required", recoverable: false } };
  }

  // Fallback to content script if API not available
  if (!isNevofluxApiAvailable()) {
    console.log("[NevoFlux] browser.nevoflux not available, using content script for type_by_id");
    return await executeInContentScript(tabId, "type_by_id", params, timeout_ms);
  }

  try {
    // Get selector from element ID
    const selector = await getElementSelector(tabId, element_id);

    if (!selector) {
      return {
        success: false,
        error: { code: -1, message: `Element ID ${element_id} not found. Take a new snapshot first.`, recoverable: true },
      };
    }

    console.log(`[NevoFlux] Typing into element ${element_id} via browser.nevoflux.type('${selector}', '${text.substring(0, 20)}...')`);

    // Click to focus first
    await browser.nevoflux.click(tabId, selector);
    await new Promise(r => setTimeout(r, 100));

    // Type text character by character - uses trusted keyboard events
    const result = await browser.nevoflux.type(tabId, selector, text);

    if (result.success === false) {
      return result;
    }

    // Press Enter if requested
    if (press_enter) {
      await new Promise(r => setTimeout(r, 100));
      // Focus again in case it was lost
      await browser.nevoflux.focus(tabId, selector);
      await new Promise(r => setTimeout(r, 50));
      await browser.nevoflux.keyPress(tabId, "Enter");
    }

    return {
      success: true,
      result: { element_id, selector, typed: text, enter_pressed: press_enter, method: "nevoflux_api" },
    };
  } catch (error) {
    console.error("[NevoFlux] nevoflux.type failed:", error.message);
    // Fallback to content script
    return await executeInContentScript(tabId, "type_by_id", params, timeout_ms);
  }
}

/**
 * Press key via browser.nevoflux.keyPress() - uses trusted keyboard events
 */
async function executeKeyPressViaApi(tabId, params) {
  const { key, modifiers = [] } = params;
  if (!key) {
    return { success: false, error: { code: -1, message: "key required", recoverable: false } };
  }

  try {
    const result = await browser.nevoflux.keyPress(tabId, key, { modifiers });
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    // If the error is "Actor destroyed", it means the page navigated after the key press
    // This is expected for Enter key on forms, so treat it as success
    if (error.message && error.message.includes("destroyed")) {
      console.log(`[NevoFlux] keyPress '${key}' triggered navigation (Actor destroyed) - treating as success`);
      return { success: true, result: { key, navigated: true } };
    }
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Get page content as Markdown via browser.nevoflux.getMarkdown()
 */
async function executeGetMarkdownViaApi(tabId, params) {
  try {
    const result = await browser.nevoflux.getMarkdown(tabId, params);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

// =============================================================================
// Content Script Fallback
// =============================================================================

/**
 * Execute action in content script
 */
async function executeInContentScript(tabId, action, params, timeout_ms) {
  // Helper function to send message to content script
  const sendActionMessage = () => {
    return browser.tabs.sendMessage(tabId, {
      type: "browser_tool_action",
      action,
      params,
    });
  };

  // Helper function to inject content script
  const injectContentScript = async () => {
    console.log("[NevoFlux] Injecting content script into tab", tabId);
    await browser.scripting.executeScript({
      target: { tabId },
      files: ["content/content.js"],
    });
    // Small delay to let the script initialize
    await new Promise((r) => setTimeout(r, 100));
  };

  // Refactor: avoid async in Promise constructor to ensure proper timeout cleanup
  let timeoutId = null;
  let resolved = false;

  try {
    // First try: send message to existing content script
    const response = await Promise.race([
      sendActionMessage(),
      new Promise((_, reject) =>
        (timeoutId = setTimeout(() => reject(new Error(`Action timed out after ${timeout_ms}ms`)), timeout_ms))
      ),
    ]);

    resolved = true;
    if (timeoutId) clearTimeout(timeoutId);
    if (response && response.success !== undefined) {
      return response;
    } else {
      return { success: true, result: response };
    }
  } catch (error) {
    // Content script not loaded, try injecting it
    console.warn("[NevoFlux] Content script not responding, injecting:", error.message);

    try {
      // Calculate remaining time after first attempt
      const remainingMs = Math.max(0, timeout_ms - 500);

      await injectContentScript();

      // Retry the action after injection with remaining time
      const response = await Promise.race([
        sendActionMessage(),
        new Promise((_, reject) =>
          (timeoutId = setTimeout(() => reject(new Error(`Action timed out after ${timeout_ms}ms`)), remainingMs))
        ),
      ]);

      resolved = true;
      if (timeoutId) clearTimeout(timeoutId);
      if (response && response.success !== undefined) {
        return response;
      } else {
        return { success: true, result: response };
      }
    } catch (injectError) {
      if (timeoutId) clearTimeout(timeoutId);
      console.error("[NevoFlux] Failed to execute in content script:", injectError.message);
      return {
        success: false,
        error: { code: -1, message: `Content script error: ${injectError.message}`, recoverable: true },
      };
    }
  }
}

// =============================================================================
// Message Listener (Background API)
// =============================================================================

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
  const msgType = message.type;

  console.log("[NevoFlux] Background received:", msgType);

  // Handle Background API calls ("bg:" prefix)
  if (msgType && msgType.startsWith("bg:")) {
    return handleBackgroundAPI(msgType, message, sendResponse);
  }

  // Handle legacy ping/pong
  if (msgType === MessageTypes.PING) {
    sendResponse({ type: MessageTypes.PONG, payload: { timestamp: message.payload?.timestamp } });
    channelManager.broadcastConnectionStatus();
    return;
  }

  // Handle legacy tab context request
  if (msgType === MessageTypes.REQUEST_TAB_CONTEXT) {
    getActiveTabContext().then((context) => {
      broadcastToSidebar({
        type: MessageTypes.TAB_CONTEXT_UPDATE,
        payload: context,
      });
    });
    sendResponse({ success: true });
    return;
  }

  // Ignore other messages (Sidebar handles them)
  return false;
});

/**
 * Handle Background API calls
 */
function handleBackgroundAPI(apiType, message, sendResponse) {
  switch (apiType) {
    case BackgroundAPI.CONNECT:
      channelManager.connect();
      sendResponse({ success: true });
      break;

    case BackgroundAPI.DISCONNECT:
      channelManager.disconnect();
      sendResponse({ success: true });
      break;

    case BackgroundAPI.GET_STATUS:
      sendResponse(channelManager.getStatus());
      break;

    case BackgroundAPI.MCP_ENABLE:
      channelManager.setMcpEnabled(true);
      sendResponse({ success: true });
      break;

    case BackgroundAPI.MCP_DISABLE:
      channelManager.setMcpEnabled(false);
      sendResponse({ success: true });
      break;

    case BackgroundAPI.SEND_TO_AGENT:
      const sent = channelManager.sendToAgent(message.payload);
      sendResponse({ success: sent });
      break;

    case BackgroundAPI.EXEC_TOOL:
      executeBrowserTool(message.payload, "sidebar")
        .then((result) => sendResponse(result))
        .catch((err) => sendResponse({
          success: false,
          error: { code: -1, message: err.message, recoverable: true },
        }));
      return true; // Keep sendResponse valid for async

    case BackgroundAPI.GET_TAB_CONTEXT:
      getActiveTabContext()
        .then((ctx) => sendResponse(ctx))
        .catch(() => sendResponse(null));
      return true; // Keep sendResponse valid for async

    default:
      console.warn("[NevoFlux] Unknown Background API:", apiType);
      sendResponse({ success: false, error: "Unknown API" });
  }
}

// =============================================================================
// Event Listeners
// =============================================================================

// Store listener references for cleanup to prevent memory leaks
const tabEventListeners = {
  onActivated: null,
  onUpdated: null,
};

// Update tab context when active tab changes
tabEventListeners.onActivated = async (activeInfo) => {
  const context = await getActiveTabContext();
  broadcastToSidebar({
    type: MessageTypes.TAB_CONTEXT_UPDATE,
    payload: context,
  });
};
browser.tabs.onActivated.addListener(tabEventListeners.onActivated);

// Update tab context when tab URL changes
tabEventListeners.onUpdated = async (tabId, changeInfo, tab) => {
  if (changeInfo.status === "complete" || changeInfo.url) {
    const tabs = await browser.tabs.query({ active: true, currentWindow: true });
    if (tabs[0]?.id === tabId) {
      const context = await getActiveTabContext();
      broadcastToSidebar({
        type: MessageTypes.TAB_CONTEXT_UPDATE,
        payload: context,
      });
    }
  }
};
browser.tabs.onUpdated.addListener(tabEventListeners.onUpdated);

// Cleanup function to remove event listeners (call on extension unload if needed)
function cleanupTabEventListeners() {
  if (tabEventListeners.onActivated) {
    browser.tabs.onActivated.removeListener(tabEventListeners.onActivated);
    tabEventListeners.onActivated = null;
  }
  if (tabEventListeners.onUpdated) {
    browser.tabs.onUpdated.removeListener(tabEventListeners.onUpdated);
    tabEventListeners.onUpdated = null;
  }
  console.log("[NevoFlux] Tab event listeners cleaned up");
}

// =============================================================================
// Initialization
// =============================================================================

console.log("[NevoFlux] Background script initialized (Protocol v5.0 - 2-channel architecture)");
console.log("[NevoFlux] Channels: Chat (com.nevoflux.agent), MCP (com.nevoflux.agent.mcp)");
console.log("[NevoFlux] API namespace: bg:*");
