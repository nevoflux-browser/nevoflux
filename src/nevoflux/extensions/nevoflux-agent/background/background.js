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
 * Protocol Version: 4.0 (4-channel architecture)
 *
 * Channels:
 * - Input (Sidebar -> Agent): User messages, commands, authorization responses
 * - Output (Agent -> Sidebar): Responses, status updates, permission requests
 * - MCP (Bidirectional): Browser Use MCP requests/responses
 * - PageLLM (Bidirectional): Page-mode LLM calls
 */

// =============================================================================
// Channel Names (Native Messaging Application IDs)
// =============================================================================

const CHANNEL_NAMES = {
  INPUT: "com.nevoflux.agent.input",
  OUTPUT: "com.nevoflux.agent.output",
  MCP: "com.nevoflux.agent.mcp",
  PAGELLM: "com.nevoflux.agent.pagellm",
};

// =============================================================================
// Message Type Constants
// =============================================================================

const MessageTypes = {
  // Input Channel: Sidebar -> Agent
  CHAT_MESSAGE: "chat_message",
  SKILL_COMMAND: "skill_command",
  STOP_GENERATION: "stop_generation",
  PERMISSION_RESPONSE: "permission_response",
  PLUGIN_COMMAND: "plugin_command",
  SYSTEM_COMMAND: "system_command",

  // Output Channel: Agent -> Sidebar
  STREAM_CHUNK: "stream_chunk",
  STREAM_END: "stream_end",
  CONTENT_BLOCK: "content_block",
  PERMISSION_REQUEST: "permission_request",
  AGENT_STATE: "agent_state",
  ERROR: "error",
  ACCOUNT_STATUS: "account_status",
  SYSTEM_RESPONSE: "system_response",

  // MCP Channel: Bidirectional
  MCP_REQUEST: "mcp_request",
  MCP_RESPONSE: "mcp_response",

  // PageLLM Channel: Bidirectional
  PAGE_LLM_REQUEST: "page_llm_request",
  PAGE_LLM_CHUNK: "page_llm_chunk",
  PAGE_LLM_DONE: "page_llm_done",
  PAGE_LLM_ERROR: "page_llm_error",

  // System messages
  PING: "ping",
  PONG: "pong",
  CONNECTION_STATUS: "connection_status",

  // Legacy types (backwards compatibility)
  AGENT_ERROR: "agent_error",
  TAB_CONTEXT_UPDATE: "tab_context_update",
  REQUEST_TAB_CONTEXT: "request_tab_context",
  UI_ACTION: "ui_action",
};

// Message types that should be sent to Input channel
const INPUT_CHANNEL_TYPES = new Set([
  MessageTypes.CHAT_MESSAGE,
  MessageTypes.SKILL_COMMAND,
  MessageTypes.STOP_GENERATION,
  MessageTypes.PERMISSION_RESPONSE,
  MessageTypes.PLUGIN_COMMAND,
  MessageTypes.SYSTEM_COMMAND,
]);

// Message types that should be sent to MCP channel
const MCP_CHANNEL_TYPES = new Set([
  MessageTypes.MCP_REQUEST,
  MessageTypes.MCP_RESPONSE,
]);

// Message types that should be sent to PageLLM channel
const PAGELLM_CHANNEL_TYPES = new Set([
  MessageTypes.PAGE_LLM_REQUEST,
  MessageTypes.PAGE_LLM_CHUNK,
  MessageTypes.PAGE_LLM_DONE,
  MessageTypes.PAGE_LLM_ERROR,
]);

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
// Channel Manager Class
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

    const delay = Math.min(
      RECONNECT_CONFIG.initialDelay * Math.pow(RECONNECT_CONFIG.multiplier, this.reconnectAttempts),
      RECONNECT_CONFIG.maxDelay
    );

    console.log(`[NevoFlux] Scheduling ${this.displayName} reconnect in ${delay}ms (attempt ${this.reconnectAttempts + 1})`);

    this.reconnectTimer = setTimeout(() => {
      this.reconnectAttempts++;
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
   * @param {Object} message - Message to send
   * @returns {boolean} - Whether the message was sent
   */
  send(message) {
    if (!this.port) {
      console.warn(`[NevoFlux] Cannot send to ${this.displayName} - not connected`);
      return false;
    }

    try {
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
// Channel Manager
// =============================================================================

/**
 * Manages all 4 native messaging channels
 */
class ChannelManager {
  constructor() {
    // Input channel: Sidebar -> Agent (always connected)
    this.input = new NativeChannel(
      CHANNEL_NAMES.INPUT,
      "Input",
      null, // Input channel doesn't receive messages (it's one-way)
      (connected, error) => this.handleInputStatusChange(connected, error)
    );

    // Output channel: Agent -> Sidebar (always connected)
    this.output = new NativeChannel(
      CHANNEL_NAMES.OUTPUT,
      "Output",
      (message) => this.handleOutputMessage(message),
      (connected, error) => this.handleOutputStatusChange(connected, error)
    );

    // MCP channel: Browser Use MCP (bidirectional, connected based on config)
    this.mcp = new NativeChannel(
      CHANNEL_NAMES.MCP,
      "MCP",
      (message) => this.handleMcpMessage(message),
      (connected, error) => this.handleMcpStatusChange(connected, error)
    );

    // PageLLM channel: Page-mode LLM calls (bidirectional, connected on demand)
    this.pagellm = new NativeChannel(
      CHANNEL_NAMES.PAGELLM,
      "PageLLM",
      (message) => this.handlePageLlmMessage(message),
      (connected, error) => this.handlePageLlmStatusChange(connected, error)
    );

    // Track overall connection status
    this.connectionStatus = {
      input: false,
      output: false,
      mcp: false,
      pagellm: false,
    };

    // MCP enabled flag (set via configuration)
    this.mcpEnabled = false;
  }

  /**
   * Initialize core channels (Input and Output)
   * These are always connected when the extension is active
   */
  connectCoreChannels() {
    console.log("[NevoFlux] Connecting core channels...");
    this.input.connect();
    this.output.connect();
  }

  /**
   * Enable/disable MCP channel based on configuration
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
   * Connect PageLLM channel on demand
   */
  connectPageLlm() {
    if (!this.pagellm.isConnected()) {
      this.pagellm.connect();
    }
  }

  /**
   * Disconnect PageLLM channel when no longer needed
   */
  disconnectPageLlm() {
    if (this.pagellm.isConnected()) {
      this.pagellm.disconnect();
    }
  }

  /**
   * Disconnect all channels
   */
  disconnectAll() {
    this.input.disconnect();
    this.output.disconnect();
    this.mcp.disconnect();
    this.pagellm.disconnect();
  }

  /**
   * Route a message to the appropriate channel
   * @param {Object} message - Message with type field
   * @returns {boolean} - Whether the message was sent
   */
  routeMessage(message) {
    const msgType = message.type;

    // Route to Input channel
    if (INPUT_CHANNEL_TYPES.has(msgType)) {
      return this.sendToInput(message);
    }

    // Route to MCP channel
    if (MCP_CHANNEL_TYPES.has(msgType)) {
      return this.sendToMcp(message);
    }

    // Route to PageLLM channel
    if (PAGELLM_CHANNEL_TYPES.has(msgType)) {
      return this.sendToPageLlm(message);
    }

    // Default: send to Input channel (for unknown types)
    console.warn(`[NevoFlux] Unknown message type ${msgType}, routing to Input channel`);
    return this.sendToInput(message);
  }

  /**
   * Send message to Input channel
   */
  sendToInput(message) {
    if (!this.input.isConnected()) {
      console.warn("[NevoFlux] Input channel not connected, attempting to connect...");
      this.input.connect();
    }
    return this.input.send(message);
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
      console.warn("[NevoFlux] MCP channel not connected, attempting to connect...");
      this.mcp.connect();
    }
    return this.mcp.send(message);
  }

  /**
   * Send message to PageLLM channel
   */
  sendToPageLlm(message) {
    if (!this.pagellm.isConnected()) {
      console.warn("[NevoFlux] PageLLM channel not connected, attempting to connect...");
      this.pagellm.connect();
    }
    return this.pagellm.send(message);
  }

  // ---------------------------------------------------------------------------
  // Message Handlers
  // ---------------------------------------------------------------------------

  handleOutputMessage(message) {
    // Forward all Output channel messages to sidebar
    broadcastToSidebar(message);
  }

  handleMcpMessage(message) {
    // MCP messages may need to be routed to:
    // 1. Browser Use API (via ext-nevoflux actor)
    // 2. Sidebar for status display
    const msgType = message.type;

    if (msgType === MessageTypes.MCP_REQUEST) {
      // MCP request from external agent -> execute via Browser Use API
      handleMcpRequest(message.payload);
    } else if (msgType === MessageTypes.MCP_RESPONSE) {
      // MCP response -> forward to sidebar or back to native agent
      broadcastToSidebar(message);
    } else {
      // Unknown MCP message type
      console.warn(`[NevoFlux] Unknown MCP message type: ${msgType}`);
    }
  }

  handlePageLlmMessage(message) {
    // PageLLM messages are for browser-based LLM calls
    const msgType = message.type;

    if (msgType === MessageTypes.PAGE_LLM_REQUEST) {
      // Request to make an LLM call via page mode
      handlePageLlmRequest(message.payload);
    } else {
      // Response messages (chunk, done, error) -> forward to native agent
      // These would come from the page content script, not native agent
      console.log(`[NevoFlux] PageLLM response: ${msgType}`);
    }
  }

  // ---------------------------------------------------------------------------
  // Status Change Handlers
  // ---------------------------------------------------------------------------

  handleInputStatusChange(connected, error) {
    this.connectionStatus.input = connected;
    this.broadcastConnectionStatus();
  }

  handleOutputStatusChange(connected, error) {
    this.connectionStatus.output = connected;
    this.broadcastConnectionStatus();
  }

  handleMcpStatusChange(connected, error) {
    this.connectionStatus.mcp = connected;
    this.broadcastConnectionStatus();
  }

  handlePageLlmStatusChange(connected, error) {
    this.connectionStatus.pagellm = connected;
    this.broadcastConnectionStatus();
  }

  /**
   * Broadcast overall connection status to sidebar
   */
  broadcastConnectionStatus() {
    // Core channels (input + output) determine "connected" status
    const coreConnected = this.connectionStatus.input && this.connectionStatus.output;

    broadcastToSidebar({
      type: MessageTypes.CONNECTION_STATUS,
      payload: {
        connected: coreConnected,
        channels: { ...this.connectionStatus },
      },
    });
  }

  /**
   * Get current connection status
   */
  getStatus() {
    return {
      connected: this.connectionStatus.input && this.connectionStatus.output,
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
// PageLLM Request Handler
// =============================================================================

/**
 * Handle PageLLM request
 * Routes to appropriate LLM page (claude.ai, chat.openai.com, etc.)
 */
async function handlePageLlmRequest(payload) {
  const { request_id, provider, payload: openAiRequest } = payload;
  console.log(`[NevoFlux] PageLLM request for ${provider}:`, openAiRequest.model);

  // This would be implemented to:
  // 1. Find or open the appropriate LLM page
  // 2. Inject input and trigger response
  // 3. Extract streaming response
  // 4. Send back through PageLLM channel

  // For now, send an error indicating not implemented
  channelManager.sendToPageLlm({
    type: MessageTypes.PAGE_LLM_ERROR,
    payload: {
      request_id,
      error: {
        code: "NOT_IMPLEMENTED",
        message: "Page LLM mode is not yet implemented",
      },
    },
  });
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
// Message Listener (from Sidebar)
// =============================================================================

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
  console.log("[NevoFlux] Background received:", message.type, message);

  const msgType = message.type;

  // Handle Ping/Pong for connection check
  if (msgType === MessageTypes.PING) {
    sendResponse({ type: MessageTypes.PONG, payload: { timestamp: message.payload?.timestamp } });

    // Also report connection status
    const status = channelManager.getStatus();
    broadcastToSidebar({
      type: MessageTypes.CONNECTION_STATUS,
      payload: status,
    });
    return;
  }

  // Handle tab context request locally
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

  // Handle connection management commands
  if (msgType === "connect_channels") {
    channelManager.connectCoreChannels();
    sendResponse({ success: true });
    return;
  }

  if (msgType === "disconnect_channels") {
    channelManager.disconnectAll();
    sendResponse({ success: true });
    return;
  }

  if (msgType === "enable_mcp") {
    channelManager.setMcpEnabled(message.payload?.enabled ?? true);
    sendResponse({ success: true });
    return;
  }

  if (msgType === "connect_pagellm") {
    channelManager.connectPageLlm();
    sendResponse({ success: true });
    return;
  }

  if (msgType === "disconnect_pagellm") {
    channelManager.disconnectPageLlm();
    sendResponse({ success: true });
    return;
  }

  if (msgType === "get_connection_status") {
    sendResponse(channelManager.getStatus());
    return;
  }

  // Route message to appropriate native channel
  const sent = channelManager.routeMessage(message);

  if (!sent) {
    // Failed to send - notify sidebar of error
    broadcastToSidebar({
      type: MessageTypes.ERROR,
      payload: {
        session_id: message.payload?.session_id || "",
        error_id: `send_error_${Date.now()}`,
        level: "error",
        code: "SEND_FAILED",
        message: "Failed to send message to native agent",
        recoverable: true,
      },
    });
  }

  sendResponse({ success: sent });
});

// =============================================================================
// Event Listeners
// =============================================================================

// Update tab context when active tab changes
browser.tabs.onActivated.addListener(async (activeInfo) => {
  const context = await getActiveTabContext();
  broadcastToSidebar({
    type: MessageTypes.TAB_CONTEXT_UPDATE,
    payload: context,
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
        payload: context,
      });
    }
  }
});

// =============================================================================
// Initialization
// =============================================================================

// Connect core channels on script load
// Note: We use lazy connection - channels connect when first message is sent
// This prevents errors if native agent is not installed
console.log("[NevoFlux] Background script initialized (Protocol v4.0 - 4-channel architecture)");
console.log("[NevoFlux] Channels will connect on first message or explicit connect command");
