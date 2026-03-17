/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

// Immediate debug log to verify script is loading
console.log('[NevoFlux] Background script starting...');

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
  CHAT: 'com.nevoflux.agent', // Chat channel (bidirectional)
  MCP: 'com.nevoflux.agent.mcp', // MCP channel (bidirectional)
};

// =============================================================================
// Background API (Sidebar callable, "bg:" prefix)
// =============================================================================

const BackgroundAPI = {
  // Channel management
  CONNECT: 'bg:connect',
  DISCONNECT: 'bg:disconnect',
  GET_STATUS: 'bg:get_status',

  // MCP channel management
  MCP_ENABLE: 'bg:mcp_enable',
  MCP_DISABLE: 'bg:mcp_disable',

  // Send message to Native Agent
  SEND_TO_AGENT: 'bg:send_to_agent',

  // Browser tool execution
  EXEC_TOOL: 'bg:exec_tool',

  // Tab context
  GET_TAB_CONTEXT: 'bg:get_tab_context',

  // Sidebar control
  SIDEBAR_CLOSE: 'bg:sidebar_close',
  SIDEBAR_OPEN: 'bg:sidebar_open',
  SIDEBAR_SET_WIDTH: 'bg:sidebar_set_width',

  // Settings (ContentStore)
  GET_SETTINGS: 'bg:get_settings',
};

// =============================================================================
// Message Type Constants
// =============================================================================

const MessageTypes = {
  // Sidebar -> Agent
  CHAT_MESSAGE: 'chat_message',
  SKILL_COMMAND: 'skill_command',
  STOP_GENERATION: 'stop_generation',
  PERMISSION_RESPONSE: 'permission_response',
  PLUGIN_COMMAND: 'plugin_command',
  SYSTEM_COMMAND: 'system_command',
  BROWSER_TOOL_RESPONSE: 'browser_tool_response',

  // Agent -> Sidebar
  STREAM_CHUNK: 'stream_chunk',
  STREAM_END: 'stream_end',
  CONTENT_BLOCK: 'content_block',
  PERMISSION_REQUEST: 'permission_request',
  AGENT_STATE: 'agent_state',
  ERROR: 'error',
  ACCOUNT_STATUS: 'account_status',
  SYSTEM_RESPONSE: 'system_response',
  BROWSER_TOOL_REQUEST: 'browser_tool_request',

  // MCP Channel
  MCP_REQUEST: 'mcp_request',
  MCP_RESPONSE: 'mcp_response',

  // System messages
  PING: 'ping',
  PONG: 'pong',
  CONNECTION_STATUS: 'connection_status',

  // AskUser interaction
  ASK_USER_REQUEST: 'ask_user_request',
  ASK_USER_RESPONSE: 'ask_user_response',

  // Artifact streaming (Agent -> Background -> ContentStore -> Sidebar)
  ARTIFACT_START: 'artifact_start',
  ARTIFACT_DELTA: 'artifact_delta',
  ARTIFACT_COMPLETE: 'artifact_complete',

  // Legacy types (backwards compatibility)
  TAB_CONTEXT_UPDATE: 'tab_context_update',
  REQUEST_TAB_CONTEXT: 'request_tab_context',
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
// ContentStore Persistence Configuration
// =============================================================================

const CONTENT_STORE_DEBOUNCE_MS = 1000; // 1 second per-key debounce
const CONTENT_STORE_MAX_VALUE_SIZE = 500_000; // 500KB max value size
const contentStoreDebounceTimers = new Map(); // key -> timeoutId

// Pending agent:command bridge requests: requestId → bridgeId
const pendingAgentCommands = new Map();

// Cleanup stale pending agent commands after 30s
setInterval(() => {
  const now = Date.now();
  for (const [reqId, bridgeId] of pendingAgentCommands) {
    const ts = parseInt(reqId.split('_')[1], 10);
    if (now - ts > 30000) {
      pendingAgentCommands.delete(reqId);
      browser.nevoflux
        .bridgeRespond(bridgeId, {
          success: false,
          error: { code: 'TIMEOUT', message: 'Agent command timed out' },
        })
        .catch(() => {});
    }
  }
}, 10000);

// Canvas tab tracking: reuse the same tab for artifacts within a conversation
let _canvasTabId = null;

// Artifact delta buffers: accumulate deltas synchronously to avoid read-modify-write races.
// Each key is an artifact ID, value is the accumulated content string.
const _artifactDeltaBuffers = new Map();

// Per-artifact operation queue: ensures createArtifact completes before updateArtifact runs.
// Each key is an artifact ID, value is a Promise representing the last queued operation.
const _artifactOpQueues = new Map();

// Track artifacts created via streaming protocol (artifact_start) to deduplicate
// against create_artifact tool calls which arrive with different IDs.
// Map<title, { id, timestamp }> — cleaned up after 60s.
const _streamedArtifacts = new Map();

/**
 * Queue an async operation for a specific artifact so operations execute serially.
 * This prevents race conditions where ARTIFACT_COMPLETE's updateArtifact runs
 * before ARTIFACT_START's createArtifact has finished.
 */
function queueArtifactOp(artifactId, operation) {
  const prev = _artifactOpQueues.get(artifactId) || Promise.resolve();
  const next = prev.then(operation).catch((err) => {
    console.error(`[NevoFlux] Artifact op failed for ${artifactId}:`, err);
  });
  _artifactOpQueues.set(artifactId, next);
  return next;
}

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

  console.log(
    `[NevoFlux] Chunked message into ${totalChunks} chunks (original: ${json.length} bytes)`
  );
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
    return message && typeof message.__chunk === 'object' && message.__chunk !== null;
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
      let fullBase64 = '';
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

// Canvas agent:chat session tracking
// Maps sessionId → { active: true, messageId: string }
const canvasSessions = new Map();

// Currently active canvas session.  Set when agent:chat sends a message,
// cleared when agent_state goes idle/error.  Used as fallback to forward
// agent messages that don't carry session_id in their payload (e.g.
// stream_chunk, agent_state).
let _activeCanvasSessionId = null;

// Debounce timer for canvas session completion.
// Native agent may not send agent_state for canvas sessions, so we detect
// completion from stream_chunk done markers instead.
let _canvasSessionEndTimer = null;

/**
 * End a canvas session: push session:end to the iframe, clean up tracking.
 */
function _endCanvasSession(sessionId, status) {
  if (_canvasSessionEndTimer) {
    clearTimeout(_canvasSessionEndTimer);
    _canvasSessionEndTimer = null;
  }
  browser.nevoflux
    .bridgePush(sessionId, {
      type: 'session:end',
      payload: { session_id: sessionId, status },
    })
    .catch(() => {});
  canvasSessions.delete(sessionId);
  if (_activeCanvasSessionId === sessionId) {
    _activeCanvasSessionId = null;
  }
}

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
              console.log(
                `[NevoFlux] ${this.displayName} received (reassembled):`,
                reassembled.type || reassembled
              );
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
          const errorMsg = port.error ? port.error.message || String(port.error) : 'null';
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

      // Schedule retry on initial connection failure (e.g., native host not yet registered)
      if (!this.isIntentionalDisconnect) {
        this.scheduleReconnect();
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
      console.error(
        `[NevoFlux] Max reconnection attempts (${MAX_RECONNECT_ATTEMPTS}) reached for ${this.displayName}`
      );
      this.onStatusChange?.(false);
      return;
    }

    this.cancelReconnect();

    // Increment before scheduling to avoid race condition with concurrent reconnect attempts
    this.reconnectAttempts++;

    const delay = Math.min(
      RECONNECT_CONFIG.initialDelay *
        Math.pow(RECONNECT_CONFIG.multiplier, this.reconnectAttempts - 1),
      RECONNECT_CONFIG.maxDelay
    );

    console.log(
      `[NevoFlux] Scheduling ${this.displayName} reconnect in ${delay}ms (attempt ${this.reconnectAttempts})`
    );

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
        console.log(
          `[NevoFlux] ${this.displayName} sent ${chunks.length} chunks for:`,
          message.type || message,
          message
        );
        return true;
      }

      // Send directly for small messages
      this.port.postMessage(message);
      console.log(`[NevoFlux] ${this.displayName} sent:`, message.type || message, message);
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
// Window-Session Mapping
// =============================================================================

/**
 * Get session ID for a window, creating one if it doesn't exist
 * @param {number} windowId - The window ID
 * @returns {Promise<string>} - The session ID
 */
async function getWindowSession(windowId) {
  try {
    const sessionId = await browser.sessions.getWindowValue(windowId, 'nevoflux_session_id');
    if (sessionId) {
      console.log(`[NevoFlux] Window ${windowId} has session: ${sessionId}`);
      return sessionId;
    }
  } catch (e) {
    console.warn('[NevoFlux] browser.sessions.getWindowValue failed:', e);
  }
  const newId = `sess-${Date.now().toString(16)}-${Math.random().toString(16).slice(2, 10)}`;
  await setWindowSession(windowId, newId);
  console.log(`[NevoFlux] Window ${windowId} assigned new session: ${newId}`);
  return newId;
}

/**
 * Set session ID for a window
 * @param {number} windowId - The window ID
 * @param {string} sessionId - The session ID to set
 * @returns {Promise<void>}
 */
async function setWindowSession(windowId, sessionId) {
  try {
    await browser.sessions.setWindowValue(windowId, 'nevoflux_session_id', sessionId);
    console.log(`[NevoFlux] Window ${windowId} -> session ${sessionId}`);
  } catch (e) {
    console.warn('[NevoFlux] browser.sessions.setWindowValue failed:', e);
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
      'Chat',
      (msg) => this.handleChatMessage(msg),
      (connected, error) => this.handleChatStatusChange(connected, error)
    );

    // MCP channel: Browser Use MCP (bidirectional)
    this.mcp = new NativeChannel(
      CHANNEL_NAMES.MCP,
      'MCP',
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
    console.log('[NevoFlux] Connecting Chat channel...');
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
      console.warn('[NevoFlux] Chat channel not connected, attempting to connect...');
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
        console.warn('[NevoFlux] MCP channel is disabled');
        return false;
      }
      this.mcp.connect();
    }
    return this.mcp.send(message);
  }

  /**
   * Handle messages from Chat channel
   * Artifact messages are intercepted and stored via ContentStore.
   * All messages are also broadcast to Sidebar.
   */
  handleChatMessage(message) {
    console.log('[NevoFlux] Chat channel received:', message.type);

    const msgType = message.type;

    // Intercept artifact streaming messages (start/delta/complete protocol)
    if (
      msgType === MessageTypes.ARTIFACT_START ||
      msgType === MessageTypes.ARTIFACT_DELTA ||
      msgType === MessageTypes.ARTIFACT_COMPLETE
    ) {
      handleArtifactMessage(message).catch((err) => {
        console.error('[NevoFlux] Artifact handling failed:', err);
      });
    }

    // Intercept create_artifact tool calls inside stream_chunk messages.
    // Process the artifact in background, then strip the large arguments
    // from the forwarded message so WASM deserialization doesn't choke on
    // 10KB+ of inline HTML content in the tool_call arguments.
    if (msgType === MessageTypes.STREAM_CHUNK) {
      const toolCalls = message.payload?.tool_calls;
      if (Array.isArray(toolCalls) && toolCalls.length > 0) {
        const toolNames = toolCalls.map((tc) => tc.name).join(', ');
        console.log(
          `[NevoFlux] stream_chunk tool_calls: [${toolNames}] (count=${toolCalls.length})`
        );
        for (const tc of toolCalls) {
          if (tc.name === 'create_artifact') {
            console.log(`[NevoFlux] Found create_artifact tool call in stream_chunk: id=${tc.id}`);
            handleCreateArtifactToolCall(tc).catch((err) => {
              console.error('[NevoFlux] create_artifact tool call failed:', err);
            });
          }
        }
        // Replace the message with a lightweight copy for the sidebar.
        // Keep tool name/id for ActivityFeed but strip large arguments.
        message = structuredClone(message);
        message.payload.tool_calls = toolCalls.map((tc) => ({
          id: tc.id,
          name: tc.name,
          arguments:
            typeof tc.arguments === 'string'
              ? tc.arguments.length > 512
                ? tc.arguments.slice(0, 512)
                : tc.arguments
              : JSON.stringify({
                  title: tc.arguments?.title || '',
                  type: tc.arguments?.type || '',
                }),
        }));
      }
    }

    // Intercept agent:command responses (bridge → agent → response)
    if (msgType === MessageTypes.SYSTEM_RESPONSE) {
      const reqId = message.payload?.request_id;
      const bridgeId = pendingAgentCommands.get(reqId);
      if (bridgeId) {
        pendingAgentCommands.delete(reqId);
        browser.nevoflux.bridgeRespond(bridgeId, message.payload).catch((err) => {
          console.error('[NevoFlux] agent:command bridgeRespond failed:', err);
        });
        // Cache status response for first-launch detection
        if (message.payload?.command === 'status' && message.payload?.success) {
          const statusData = message.payload.data;
          browser.storage.local.set({
            nevoflux_last_status: {
              first_run: statusData.first_run,
              has_configured_provider: statusData.has_configured_provider,
              timestamp: Date.now(),
            },
          }).catch((err) => {
            console.warn('[NevoFlux] Failed to cache status:', err);
          });
        }
        // Don't fall through to sidebar broadcast for this response
        return;
      }
    }

    // Intercept content_store responses
    if (msgType === MessageTypes.SYSTEM_RESPONSE) {
      const payload = message.payload;
      const cmd = payload?.command;
      if (cmd === 'content_store.load' && payload?.success) {
        const entries = payload.data?.entries || [];
        console.log(`[NevoFlux] Content store load response: ${entries.length} entries`);
        if (entries.length > 0) {
          browser.nevoflux
            .contentStoreLoad(entries)
            .then(() => {
              // Notify sidebar that ContentStore has been hydrated so it can re-fetch settings
              console.log('[NevoFlux] ContentStore hydrated, notifying sidebar');
              broadcastToSidebar({
                type: MessageTypes.SYSTEM_RESPONSE,
                payload: { command: 'content_store.loaded', success: true },
              });
            })
            .catch((err) => {
              console.error('[NevoFlux] Content store load failed:', err);
            });
        }
      } else if (cmd === 'artifact.get' && payload?.success) {
        const art = payload.data;
        if (art && art.id) {
          console.log(`[NevoFlux] artifact.get response: hydrating ContentStore for ${art.id}`);
          const artifactObj = {
            id: art.id,
            type: art.content_type || 'text/html',
            title: art.title || 'Untitled',
            code: art.content || '',
            state: 'complete',
          };
          if (art.files) {
            artifactObj.files = art.files;
            artifactObj.type = 'project';
          }
          if (art.entry) {
            artifactObj.entry = art.entry;
          }
          browser.nevoflux.createArtifact(artifactObj).catch((err) => {
            console.error('[NevoFlux] Failed to hydrate artifact:', err);
          });
        }
      } else if (
        (cmd === 'content_store.set' || cmd === 'content_store.delete') &&
        !payload?.success
      ) {
        console.warn(`[NevoFlux] Content store persist failed: ${cmd}`, payload?.error);
      }
    }

    // Forward to canvas sessions.
    // Try matching by session_id in payload first; fall back to
    // _activeCanvasSessionId for messages that don't carry session_id
    // (e.g. stream_chunk, agent_state).
    let canvasSessionId = message.payload?.session_id;
    if (!canvasSessionId || !canvasSessions.has(canvasSessionId)) {
      canvasSessionId = _activeCanvasSessionId;
    }
    if (canvasSessionId && canvasSessions.has(canvasSessionId)) {
      // Push to canvas via bridgePush
      browser.nevoflux.bridgePush(canvasSessionId, message).catch((err) => {
        console.error(`[NevoFlux] bridgePush failed for session ${canvasSessionId}:`, err);
      });

      // --- Detect session completion ---
      // Primary: explicit agent_state idle/error
      if (msgType === 'agent_state') {
        const status = message.payload?.state || message.payload?.status;
        if (status === 'idle' || status === 'error') {
          _endCanvasSession(canvasSessionId, status);
        }
      }

      // Fallback: stream_chunk with done:true.  The native agent may not send
      // agent_state for canvas sessions, so we also detect completion from the
      // streaming done marker.  Use a short debounce (2 s) so that back-to-back
      // done:true chunks don't fire prematurely and an arriving agent_state can
      // still take over.
      if (msgType === 'stream_chunk' && message.payload?.done === true) {
        const sid = canvasSessionId; // capture for closure
        if (_canvasSessionEndTimer) clearTimeout(_canvasSessionEndTimer);
        _canvasSessionEndTimer = setTimeout(() => {
          _canvasSessionEndTimer = null;
          if (canvasSessions.has(sid)) {
            console.log(`[NevoFlux] Canvas session ${sid} ended (stream done fallback)`);
            _endCanvasSession(sid, 'idle');
          }
        }, 2000);
      }

      // If agent starts a new streaming turn (done:false), cancel the debounce
      // timer — the agent isn't done yet (e.g. it called a tool and continues).
      if (msgType === 'stream_chunk' && message.payload?.done === false) {
        if (_canvasSessionEndTimer) {
          clearTimeout(_canvasSessionEndTimer);
          _canvasSessionEndTimer = null;
        }
      }
    }

    // Intercept browser_tool_request for actions that background.js can handle directly
    // This bypasses the Sidebar WASM round-trip for better reliability and performance
    if (msgType === MessageTypes.BROWSER_TOOL_REQUEST) {
      const payload = message.payload;
      const action = payload?.action;
      const DIRECT_ACTIONS = new Set(['read_artifact', 'edit_artifact', 'ask_user']);
      if (DIRECT_ACTIONS.has(action)) {
        console.log(`[NevoFlux] Handling ${action} directly (bypassing sidebar)`);
        executeBrowserTool(payload, 'direct')
          .then((toolResult) => {
            channelManager.sendToAgent({
              type: MessageTypes.BROWSER_TOOL_RESPONSE,
              payload: {
                request_id: payload.request_id,
                session_id: payload.session_id,
                success: toolResult.success,
                result: toolResult.success ? toolResult.result : undefined,
                error: toolResult.error || undefined,
              },
            });
          })
          .catch((err) => {
            channelManager.sendToAgent({
              type: MessageTypes.BROWSER_TOOL_RESPONSE,
              payload: {
                request_id: payload.request_id,
                session_id: payload.session_id,
                success: false,
                error: { code: -1, message: err.message || String(err), recoverable: true },
              },
            });
          });
        // Don't broadcast to sidebar — it would trigger a duplicate bg:exec_tool
        // round-trip and send a second browser_tool_response to the native agent.
        return;
      }
    }

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
  handleChatStatusChange(connected, _error) {
    this.connectionStatus.chat = connected;
    if (connected) {
      console.log('[NevoFlux] Chat channel connected (bidirectional communication ready)');

      // Load persisted ContentStore entries from Rust agent
      console.log('[NevoFlux] Loading content store from agent...');
      this.sendToAgent({
        type: MessageTypes.SYSTEM_COMMAND,
        payload: {
          command: 'content_store.load',
          request_id: `cs_load_${Date.now()}`,
          params: { prefix: '' },
        },
      });
    }
    this.broadcastConnectionStatus();
  }

  /**
   * Handle MCP channel status change
   */
  handleMcpStatusChange(connected, _error) {
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
    console.debug('[NevoFlux] Sidebar not available:', err.message);
  });
}

// =============================================================================
// ContentStore Persistence Bridge
// =============================================================================

/**
 * Listen for ContentStore changes and persist to Rust agent's SQLite.
 * Uses per-key debouncing (1s) to coalesce rapid writes (e.g. artifact streaming).
 * Wrapped in guard: browser.nevoflux API may not be available on all platforms.
 */
if (typeof browser.nevoflux !== 'undefined' && browser.nevoflux.onContentStoreChanged) {
  browser.nevoflux.onContentStoreChanged.addListener((operation, key, value) => {
    // Skip if agent not connected
    if (!channelManager.connectionStatus.chat) {
      console.debug(
        '[NevoFlux] ContentStore changed but agent not connected, skipping persist:',
        key
      );
      return;
    }

    // Clear existing debounce timer for this key
    if (contentStoreDebounceTimers.has(key)) {
      clearTimeout(contentStoreDebounceTimers.get(key));
    }

    // Debounce per key
    const timerId = setTimeout(() => {
      contentStoreDebounceTimers.delete(key);

      // Re-check connection after debounce delay
      if (!channelManager.connectionStatus.chat) {
        console.debug('[NevoFlux] Agent disconnected during debounce, skipping persist:', key);
        return;
      }

      if (operation === 'set') {
        // Guard: skip oversized values
        const serialized = JSON.stringify(value);
        if (serialized && serialized.length > CONTENT_STORE_MAX_VALUE_SIZE) {
          console.warn(
            `[NevoFlux] ContentStore value too large (${serialized.length}), skipping persist: ${key}`
          );
          return;
        }

        console.log(`[NevoFlux] Persisting: content_store.set ${key}`);
        channelManager.sendToAgent({
          type: MessageTypes.SYSTEM_COMMAND,
          payload: {
            command: 'content_store.set',
            request_id: `cs_set_${Date.now()}`,
            params: { key, value },
          },
        });
      } else if (operation === 'delete') {
        console.log(`[NevoFlux] Persisting: content_store.delete ${key}`);
        channelManager.sendToAgent({
          type: MessageTypes.SYSTEM_COMMAND,
          payload: {
            command: 'content_store.delete',
            request_id: `cs_del_${Date.now()}`,
            params: { key },
          },
        });
      }
    }, CONTENT_STORE_DEBOUNCE_MS);

    contentStoreDebounceTimers.set(key, timerId);
  });
} else {
  console.warn('[NevoFlux] browser.nevoflux API not available, ContentStore persistence disabled');
}

// =============================================================================
// Bridge Request Handler (nevoflux:// pages → background)
// =============================================================================

if (typeof browser.nevoflux !== 'undefined' && browser.nevoflux.onBridgeRequest) {
  browser.nevoflux.onBridgeRequest.addListener(async (id, type, payload) => {
    console.log(`[NevoFlux] Bridge request: ${type} (${id})`);
    let result;
    try {
      switch (type) {
        case 'exec_tool':
          result = await executeBrowserTool(payload, 'bridge');
          break;

        case 'send_to_agent': {
          const sent = channelManager.sendToAgent(payload);
          result = { success: sent };
          break;
        }

        case 'sidebar_send':
          broadcastToSidebar(payload);
          result = { success: true };
          break;

        case 'sidebar:open':
          try {
            await browser.sidebarAction.open();
            result = { success: true };
          } catch (err) {
            result = { success: false, error: { code: -1, message: err.message } };
          }
          break;

        case 'sidebar:sendMessage': {
          try {
            const messageId = `msg_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
            const injectMsg = {
              type: 'canvas_chat_inject',
              payload: {
                session_id: '',
                message_id: messageId,
                content: payload.message,
              },
            };

            // Broadcast to sidebar — sidebar adds user message + sends to agent
            try {
              await browser.runtime.sendMessage(injectMsg);
              console.log('[NevoFlux] canvas_chat_inject sent to sidebar');
            } catch (e) {
              // Sidebar not open — send directly to agent as fallback
              console.warn(
                '[NevoFlux] Sidebar not available, sending directly to agent:',
                e.message
              );
              const canvasHint = await getActiveCanvasHint();
              const msgContent = canvasHint
                ? canvasHint + '\n\n' + payload.message
                : payload.message;
              channelManager.sendToAgent({
                type: 'chat_message',
                payload: {
                  session_id: `canvas_${Date.now()}`,
                  message_id: messageId,
                  content: msgContent,
                  mode: 'chat',
                  attachments: [],
                  local_files: [],
                  tab_id: null,
                  tab_ids: [],
                },
              });
            }

            result = { success: true };
          } catch (err) {
            result = { success: false, error: { code: -1, message: err.message } };
          }
          break;
        }

        case 'agent:chat': {
          try {
            const sessionId =
              payload.sessionId || `cs_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
            const messageId = `msg_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

            // Track this canvas session
            canvasSessions.set(sessionId, { active: true, messageId });
            _activeCanvasSessionId = sessionId;

            // Always send directly to agent with canvas sessionId.
            // This ensures streaming responses carry the canvas sessionId so
            // handleChatMessage can route them back via bridgePush.
            const agentChatHint = await getActiveCanvasHint();
            const agentChatContent = agentChatHint
              ? agentChatHint + '\n\n' + payload.message
              : payload.message;
            channelManager.sendToAgent({
              type: 'chat_message',
              payload: {
                session_id: sessionId,
                message_id: messageId,
                content: agentChatContent,
                mode: 'agent',
                attachments: [],
                local_files: [],
                tab_id: null,
                tab_ids: [],
              },
            });

            // Notify sidebar for UI display only (fire-and-forget).
            // Sidebar will show the user message but NOT re-send to agent.
            try {
              await browser.runtime.sendMessage({
                type: 'canvas_chat_inject',
                payload: {
                  session_id: sessionId,
                  message_id: messageId,
                  content: payload.message,
                  source: 'canvas',
                },
              });
            } catch (_e) {
              // Sidebar not open — that's fine, agent message already sent above
            }

            result = { success: true, sessionId };
          } catch (err) {
            result = { success: false, error: { code: -1, message: err.message } };
          }
          break;
        }

        case 'agent:cancel': {
          const sessionId = payload.sessionId;
          canvasSessions.delete(sessionId);
          result = { success: true };
          break;
        }

        case 'sidebar:restoreSession': {
          try {
            // Store pending session restore for the sidebar to pick up on init
            await browser.storage.local.set({
              'pending:restoreSession': {
                sessionId: payload.sessionId,
                timestamp: Date.now(),
              },
            });
            // Open sidebar (triggers init which reads pending actions)
            await browser.sidebarAction.open();
            result = { success: true };
          } catch (err) {
            result = { success: false, error: { code: -1, message: err.message } };
          }
          break;
        }

        case 'agent:command': {
          // Forward a system_command to the native agent and wait for matching system_response.
          // The response comes back asynchronously via handleChatMessage → system_response interception.
          const { command, params: cmdParams } = payload;
          const requestId = `brcmd_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

          pendingAgentCommands.set(requestId, id);

          channelManager.sendToAgent({
            type: MessageTypes.SYSTEM_COMMAND,
            payload: { command, request_id: requestId, params: cmdParams || {} },
          });

          // Don't call bridgeRespond here — wait for system_response
          return;
        }

        case 'getCache': {
          const key = payload?.key || 'nevoflux_last_status';
          const stored = await browser.storage.local.get(key);
          result = { success: true, data: stored[key] || null };
          break;
        }

        default:
          result = { success: false, error: { code: -1, message: `Unknown bridge type: ${type}` } };
      }
    } catch (err) {
      console.error(`[NevoFlux] Bridge request error (${type}):`, err);
      result = { success: false, error: { code: -1, message: err.message } };
    }

    browser.nevoflux.bridgeRespond(id, result).catch((err) => {
      console.error(`[NevoFlux] bridgeRespond failed for ${id}:`, err);
    });
  });
} else {
  console.warn('[NevoFlux] browser.nevoflux API not available, Bridge handler disabled');
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
  console.log(`[NevoFlux] MCP request from ${source?.agent || 'unknown'}:`, jsonRpcRequest.method);

  try {
    // Get active tab to execute Browser Use API
    const tabs = await browser.tabs.query({ active: true, currentWindow: true });
    if (!tabs[0]) {
      throw new Error('No active tab');
    }

    // Send to content script which will communicate with JSWindowActor
    const result = await browser.tabs.sendMessage(tabs[0].id, {
      target: 'browser-use-api',
      type: 'mcp_execute',
      payload: jsonRpcRequest,
    });

    // Send response back through MCP channel
    channelManager.sendToMcp({
      type: MessageTypes.MCP_RESPONSE,
      payload: {
        request_id,
        payload: {
          jsonrpc: '2.0',
          id: jsonRpcRequest.id,
          result,
        },
      },
    });
  } catch (error) {
    console.error('[NevoFlux] MCP request failed:', error);

    // Send error response
    channelManager.sendToMcp({
      type: MessageTypes.MCP_RESPONSE,
      payload: {
        request_id,
        payload: {
          jsonrpc: '2.0',
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
// Single-Shot Artifact Handler
// =============================================================================

/**
 * Handle single-shot artifact message from native agent (type: "artifact").
 *
 * The native agent sends the full artifact content in one message:
 *   { type: "artifact", payload: { id, content, content_type, title, ... } }
 *
 * We convert this into:
 *   1. createArtifact() call (store in ContentStore)
 *   2. Open/reuse canvas tab in foreground
 *   3. Broadcast artifact_start to sidebar (for ArtifactCard)
 *   4. Broadcast artifact_complete to sidebar (update card state)
 */
// =============================================================================
// Artifact Streaming Handler
// =============================================================================

/**
 * Normalize MIME type or short type to canvas renderer type.
 * Daemon sends "text/html", canvas.js expects "html".
 */
function normalizeArtifactType(rawType) {
  if (!rawType) return 'html';
  const MIME_MAP = {
    'text/html': 'html',
    'text/markdown': 'markdown',
    'text/svg+xml': 'svg',
    'image/svg+xml': 'svg',
    'application/javascript': 'react',
    'text/jsx': 'react',
    'application/project': 'project',
    'application/project+json': 'project',
  };
  if (MIME_MAP[rawType]) return MIME_MAP[rawType];
  // Already a short type
  if (['html', 'react', 'markdown', 'svg', 'mermaid', 'project'].includes(rawType)) return rawType;
  // Fallback: try to extract subtype from MIME
  const parts = rawType.split('/');
  if (parts.length === 2) return parts[1];
  return rawType;
}

/**
 * Handle artifact streaming messages from the native agent.
 *
 * Flow:
 *   artifact_start  → createArtifact (state: "streaming")
 *   artifact_delta  → updateArtifact (append code)
 *   artifact_complete → updateArtifact (state: "complete")
 */
async function handleArtifactMessage(message) {
  const { type, payload } = message;
  console.log(
    `[NevoFlux] handleArtifactMessage: type=${type}, payloadKeys=${payload ? Object.keys(payload).join(',') : 'null'}`
  );

  switch (type) {
    case MessageTypes.ARTIFACT_START: {
      const { id, content_type, title, source, permissions, files, entry, options } = payload;
      console.log(
        `[NevoFlux] ARTIFACT_START: id=${id}, content_type=${content_type}, files=${!!files}, filesCount=${files ? Object.keys(files).length : 0}`
      );

      // Initialize delta buffer for this artifact
      _artifactDeltaBuffers.set(id, '');

      // Normalize MIME type to short canvas type
      // Daemon sends "text/html", canvas.js expects "html"
      const normalizedType = normalizeArtifactType(content_type);
      console.log(`[NevoFlux] ARTIFACT_START: normalizedType=${normalizedType}`);

      const createOptions = {
        id,
        type: normalizedType,
        title: title || 'Untitled',
        code: '',
        state: 'streaming',
        source: source || 'agent',
        permissions: permissions || [],
      };

      // Multi-file project support
      if (files) {
        createOptions.files = files;
        createOptions.entry = entry;
        createOptions.options = options;
      }

      // Queue the createArtifact call so subsequent operations (delta/complete)
      // wait for it to finish before running.
      console.log(`[NevoFlux] ARTIFACT_START: queuing createArtifact for ${id}`);
      await queueArtifactOp(id, async () => {
        console.log(`[NevoFlux] ARTIFACT_START: executing createArtifact for ${id}`);
        try {
          await browser.nevoflux.createArtifact(createOptions);
          console.log(`[NevoFlux] Artifact ${id} created in ContentStore (type=${normalizedType})`);

          // Track this streamed artifact for dedup against create_artifact tool calls
          const artTitle = (title || 'Untitled').toLowerCase().trim();
          _streamedArtifacts.set(artTitle, { id, timestamp: Date.now() });
          // Auto-cleanup after 60s
          setTimeout(() => _streamedArtifacts.delete(artTitle), 60000);
        } catch (createErr) {
          console.error(`[NevoFlux] createArtifact FAILED for ${id}:`, createErr);
          throw createErr;
        }
      });

      // Open canvas tab immediately — page subscribes to ContentStore updates
      // and will render content as artifact_delta messages arrive
      try {
        if (_canvasTabId != null) {
          try {
            await browser.tabs.remove(_canvasTabId);
          } catch {
            // Tab already closed — ignore
          }
          _canvasTabId = null;
        }
        const result = await browser.nevoflux.openCanvasTab(id);
        if (result?.success) {
          if (result.tabId) _canvasTabId = result.tabId;
        } else {
          console.error('[NevoFlux] openCanvasTab failed:', result?.error);
        }
      } catch (e) {
        console.error('[NevoFlux] Failed to open canvas tab:', e);
      }
      break;
    }

    case MessageTypes.ARTIFACT_DELTA: {
      const { id, delta } = payload;
      // Accumulate deltas synchronously in a local buffer to avoid
      // read-modify-write race conditions when multiple deltas arrive rapidly.
      if (!_artifactDeltaBuffers.has(id)) {
        _artifactDeltaBuffers.set(id, '');
      }
      _artifactDeltaBuffers.set(id, _artifactDeltaBuffers.get(id) + (delta || ''));
      // Queue the update to wait for createArtifact to finish first
      const buffered = _artifactDeltaBuffers.get(id);
      await queueArtifactOp(id, () => browser.nevoflux.updateArtifact(id, { code: buffered }));
      break;
    }

    case MessageTypes.ARTIFACT_COMPLETE: {
      const { id, final_code, title, files, entry } = payload;
      const bufferedLen = _artifactDeltaBuffers.get(id)?.length || 0;
      console.log(
        `[NevoFlux] ARTIFACT_COMPLETE: id=${id}, bufferedContentLen=${bufferedLen}, hasFiles=${files !== undefined}`
      );
      // Clean up delta buffer
      _artifactDeltaBuffers.delete(id);
      const updates = { state: 'complete' };
      if (final_code !== undefined) updates.code = final_code;
      if (title !== undefined) updates.title = title;
      // Multi-file project support
      if (files !== undefined) updates.files = files;
      if (entry !== undefined) updates.entry = entry;
      // Queue the update to wait for createArtifact to finish first
      await queueArtifactOp(id, async () => {
        const result = await browser.nevoflux.updateArtifact(id, updates);
        if (!result?.success) {
          console.warn(
            `[NevoFlux] ARTIFACT_COMPLETE: updateArtifact failed for ${id} (artifact not found), will rely on tool call handler`
          );
        }
      });
      break;
    }
  }
}

/**
 * Handle a create_artifact tool call from a stream_chunk message.
 *
 * The daemon may send BOTH the streaming protocol (artifact_start/delta/complete)
 * AND a create_artifact tool call for the same artifact. If the streaming protocol
 * already handled it, skip to avoid duplicate ArtifactCards and tab opens.
 */
async function handleCreateArtifactToolCall(toolCall) {
  let args = toolCall.arguments;
  const argsType = typeof args;
  const rawArgsLen = typeof args === 'string' ? args.length : JSON.stringify(args).length;
  console.log(`[NevoFlux] create_artifact: argsType=${argsType}, rawArgsLen=${rawArgsLen}`);

  if (typeof args === 'string') {
    try {
      args = JSON.parse(args);
    } catch (e) {
      // Arguments may be truncated by the model - try to salvage partial data
      console.warn(
        '[NevoFlux] create_artifact arguments truncated, attempting partial parse:',
        e.message
      );
      console.warn('[NevoFlux] create_artifact raw args (first 500):', args.substring(0, 500));
      args = extractPartialArtifactArgs(args);
      if (!args) {
        console.error(
          '[NevoFlux] Failed to extract any data from truncated create_artifact arguments'
        );
        return;
      }
      console.log(
        '[NevoFlux] create_artifact partial parse result: contentLen=' + (args.content?.length || 0)
      );
    }
  }

  const id = args.id || `art-${toolCall.id || Date.now()}`;
  const content = args.content || '';
  const title = args.title || 'Untitled';
  const rawType = args.type || args.content_type || 'html';
  const normalizedType = normalizeArtifactType(rawType);
  const files = args.files || null;
  const entry = args.entry || null;
  const isProject = normalizedType === 'project' || !!files;

  console.log(
    `[NevoFlux] create_artifact tool call: id=${id}, type=${normalizedType}, contentLen=${content.length}, isProject=${isProject}, filesCount=${files ? Object.keys(files).length : 0}`
  );

  // The daemon sends BOTH the streaming protocol (artifact_start/delta/complete)
  // AND the create_artifact tool call for the same artifact. The tool call has
  // the FULL content, so it's the authoritative source. Queue the operation to
  // ensure it runs after any pending ARTIFACT_START createArtifact completes.
  await queueArtifactOp(id, async () => {
    const existing = await browser.nevoflux.getArtifact(id).catch(() => null);
    if (existing?.success) {
      // Artifact exists from streaming protocol — update with full content from tool call
      // which is more reliable than accumulated deltas.
      const existingLen = existing.data?.content?.length || 0;
      const existingType = existing.data?.type || 'unknown';
      console.log(
        `[NevoFlux] create_artifact: artifact ${id} exists (type=${existingType}, streamedLen=${existingLen}), updating with tool call data`
      );
      const updates = { state: 'complete' };
      // For project-type: always set type, files, entry from tool call (authoritative)
      if (isProject) {
        updates.type = 'project';
        if (files) {
          updates.files = files;
          updates.entry = entry;
        }
      }
      if (content.length > 0) updates.code = content;
      await browser.nevoflux.updateArtifact(id, updates);
      return;
    }

    // Check if the streaming protocol already created this artifact (different ID, same title)
    const normalizedTitle = (title || 'Untitled').toLowerCase().trim();
    const streamedEntry = _streamedArtifacts.get(normalizedTitle);
    if (streamedEntry) {
      // Streaming protocol already created this artifact — update it with full content
      console.log(
        `[NevoFlux] create_artifact: dedup hit — streamed artifact "${normalizedTitle}" exists as ${streamedEntry.id}, updating instead of creating ${id}`
      );
      const updates = { state: 'complete' };
      if (isProject) {
        updates.type = 'project';
        if (files) {
          updates.files = files;
          updates.entry = entry;
        }
      }
      if (content.length > 0) updates.code = content;
      await browser.nevoflux.updateArtifact(streamedEntry.id, updates);
      _streamedArtifacts.delete(normalizedTitle);
      return;
    }

    // Artifact doesn't exist yet — create it with full content, mark as complete
    console.log(
      `[NevoFlux] create_artifact: creating new artifact ${id} (type=${isProject ? 'project' : normalizedType})`
    );
    const createOpts = {
      id,
      type: isProject ? 'project' : normalizedType,
      title,
      code: content,
      state: 'complete',
      source: 'agent',
      permissions: [],
    };
    if (files) {
      createOpts.files = files;
      createOpts.entry = entry;
    }
    await browser.nevoflux.createArtifact(createOpts);

    // Open canvas tab only if streaming protocol hasn't already
    try {
      if (_canvasTabId != null) {
        try {
          await browser.tabs.remove(_canvasTabId);
        } catch {}
        _canvasTabId = null;
      }
      const result = await browser.nevoflux.openCanvasTab(id);
      if (result?.success) {
        if (result.tabId) _canvasTabId = result.tabId;
      } else {
        console.error('[NevoFlux] openCanvasTab failed:', result?.error);
      }
    } catch (e) {
      console.error('[NevoFlux] Failed to open canvas tab:', e);
    }

    // Broadcast artifact_start + artifact_complete to sidebar for ArtifactCard
    const startPayload = { id, content_type: isProject ? 'project' : normalizedType, title };
    if (files) {
      startPayload.files = files;
      startPayload.entry = entry;
    }
    broadcastToSidebar({
      type: MessageTypes.ARTIFACT_START,
      payload: startPayload,
    });
    broadcastToSidebar({
      type: MessageTypes.ARTIFACT_COMPLETE,
      payload: { id, title },
    });
  });
}

/**
 * Extract partial artifact arguments from truncated JSON string.
 * When the model runs out of tokens, the JSON arguments are cut off.
 * This tries to extract whatever fields are available.
 */
function extractPartialArtifactArgs(truncatedJson) {
  const result = {};

  // Try to extract "title" field
  const titleMatch = truncatedJson.match(/"title"\s*:\s*"((?:[^"\\]|\\.)*)"/);
  if (titleMatch) result.title = titleMatch[1];

  // Try to extract "type" or "content_type" field
  const typeMatch = truncatedJson.match(/"(?:type|content_type)"\s*:\s*"((?:[^"\\]|\\.)*)"/);
  if (typeMatch) result.type = typeMatch[1];

  // Try to extract "id" field
  const idMatch = truncatedJson.match(/"id"\s*:\s*"((?:[^"\\]|\\.)*)"/);
  if (idMatch) result.id = idMatch[1];

  // Try to extract "content" field - take everything after "content":" up to the end
  const contentMatch = truncatedJson.match(/"content"\s*:\s*"([\s\S]*)$/);
  if (contentMatch) {
    let content = contentMatch[1];
    // Remove trailing incomplete escape/quote if present
    if (content.endsWith('\\')) content = content.slice(0, -1);
    if (content.endsWith('"')) content = content.slice(0, -1);
    // Unescape JSON string escapes
    try {
      content = JSON.parse('"' + content + '"');
    } catch {
      // If unescape fails, use raw content with basic unescaping
      content = content
        .replace(/\\n/g, '\n')
        .replace(/\\t/g, '\t')
        .replace(/\\"/g, '"')
        .replace(/\\\\/g, '\\');
    }
    result.content = content;
  }

  // Must have at least some content to be useful
  if (result.content || result.title) {
    console.log(
      `[NevoFlux] Extracted partial artifact: title=${result.title}, type=${result.type}, contentLen=${result.content?.length || 0}`
    );
    return result;
  }
  return null;
}

// =============================================================================
// Tab Context Helpers
// =============================================================================

/**
 * Get active tab ID
 */
async function getActiveTabId() {
  const tabs = await browser.tabs.query({ active: true, currentWindow: true });
  const tab = tabs[0];
  if (!tab) return null;

  // If the active tab is an internal page, find the best web tab instead
  const url = tab.url || '';
  if (
    url.startsWith('nevoflux://') ||
    url.startsWith('chrome://nevoflux/') ||
    url.startsWith('about:') ||
    url.startsWith('chrome://')
  ) {
    // Find the most recently accessed non-internal tab in the current window
    const allTabs = await browser.tabs.query({ currentWindow: true });
    const webTabs = allTabs.filter((t) => {
      const u = t.url || '';
      return (u.startsWith('http://') || u.startsWith('https://')) && !t.discarded;
    });
    if (webTabs.length > 0) {
      // Sort by lastAccessed descending, pick most recent
      webTabs.sort((a, b) => (b.lastAccessed || 0) - (a.lastAccessed || 0));
      return webTabs[0].id;
    }
    // Fallback: try discarded web tabs (will trigger auto-restore)
    const discardedWebTabs = allTabs.filter((t) => {
      const u = t.url || '';
      return (u.startsWith('http://') || u.startsWith('https://')) && t.discarded;
    });
    if (discardedWebTabs.length > 0) {
      discardedWebTabs.sort((a, b) => (b.lastAccessed || 0) - (a.lastAccessed || 0));
      return discardedWebTabs[0].id;
    }
    // No web tab found — return null
    return null;
  }

  return tab.id;
}

/**
 * Get tab context for a specific tab or the active tab
 * @param {number|null} tabId - Optional tab ID. If null, gets active tab.
 */
async function getTabContext(tabId = null) {
  let tab;

  if (tabId != null) {
    // Get specific tab by ID
    try {
      tab = await browser.tabs.get(tabId);
    } catch (e) {
      console.warn('[NevoFlux] Failed to get tab by ID:', tabId, e);
      tab = null;
    }
  } else {
    // Get active tab
    const tabs = await browser.tabs.query({ active: true, currentWindow: true });
    tab = tabs[0];
  }

  if (!tab) {
    return {
      tab_id: 0,
      zen_sync_id: null,
      url: '',
      title: '',
      favicon_url: null,
      status: 'complete',
    };
  }

  // Get zenSyncId from nevoflux API (includes Zen Browser's persistent tab ID)
  let zenSyncId = null;
  try {
    const tabInfo = await browser.nevoflux.getTab(tab.id);
    zenSyncId = tabInfo?.zenSyncId || null;
  } catch (e) {
    console.warn('[NevoFlux] Failed to get zenSyncId:', e);
  }

  return {
    tab_id: tab.id,
    zen_sync_id: zenSyncId,
    url: tab.url || '',
    title: tab.title || '',
    favicon_url: tab.favIconUrl || null,
    status: tab.status || 'complete',
  };
}

// Legacy alias for backward compatibility
async function getActiveTabContext() {
  return getTabContext(null);
}

/**
 * Get context hint for active canvas artifact (if any).
 * Returns a string hint to prepend to user message, or null if no canvas is active.
 */
async function getActiveCanvasHint() {
  try {
    const tabs = await browser.tabs.query({ active: true, currentWindow: true });
    const canvasTab = tabs.find((t) => t.url?.startsWith('nevoflux://canvas/'));
    if (!canvasTab) return null;

    const id = canvasTab.url.split('nevoflux://canvas/')[1];
    if (!id) return null;

    const result = await browser.nevoflux.getArtifact(id);
    if (!result?.success || result.data?.state === 'streaming') return null;

    const lines = result.data.content?.split('\n').length || 0;
    return `[Active Canvas: id="${id}", title="${result.data.title || 'Untitled'}", type="${result.data.type || 'html'}", lines=${lines}]\nYou can use browser_read_artifact and browser_edit_artifact to view or modify this artifact.`;
  } catch (e) {
    console.warn('[NevoFlux] Failed to get canvas hint:', e);
    return null;
  }
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
async function executeBrowserTool(request, caller = 'unknown') {
  const { action, params, tab_id, timeout_ms = 30000 } = request;

  // Actions that don't require an active tab
  const TAB_INDEPENDENT_ACTIONS = new Set([
    'ask_user',
    'list_tabs',
    'query_tabs',
    'web_fetch',
    'web_search',
    'cache_file',
    'read_artifact',
    'edit_artifact',
    'canvas_render',
  ]);

  // Get target tab (skip for tab-independent actions)
  let targetTabId = tab_id;
  if (!targetTabId && !TAB_INDEPENDENT_ACTIONS.has(action)) {
    targetTabId = await getActiveTabId();
    if (!targetTabId) {
      // For navigate, create a new tab if no web tab exists
      if (action === 'navigate' && params?.url) {
        const newTab = await browser.tabs.create({ url: params.url });
        return { success: true, result: { url: params.url, tab_id: newTab.id } };
      }
      return {
        success: false,
        error: {
          code: -1,
          message: 'No active web tab found. Open a web page first.',
          recoverable: true,
        },
      };
    }
  }

  // Check if browser.nevoflux API is available
  const useNevofluxApi = isNevofluxApiAvailable();
  console.log(
    `[NevoFlux] [${caller}] Executing browser tool: ${action} on tab ${targetTabId} (nevoflux API: ${useNevofluxApi})`
  );

  try {
    switch (action) {
      // Navigation
      case 'navigate':
        return await executeNavigateViaApi(targetTabId, params);

      case 'go_back':
        return await executeGoBackViaApi(targetTabId);

      case 'go_forward':
        return await executeGoForwardViaApi(targetTabId);

      // Selector-based interactions (uses trusted events via windowUtils)
      case 'click':
        return await executeClickViaApi(targetTabId, params);

      case 'type':
        return await executeTypeViaApi(targetTabId, params);

      case 'fill':
        return await executeFillViaApi(targetTabId, params);

      // Data extraction
      case 'get_content':
        return await executeGetContentViaApi(targetTabId, params);

      case 'screenshot':
        return await executeScreenshotViaApi(targetTabId, params);

      // JavaScript execution
      case 'eval_js':
        return await executeEvalJsViaApi(targetTabId, params);

      // Waiting
      case 'wait_for':
        return await executeWaitForViaApi(targetTabId, params, timeout_ms);

      // Scrolling
      case 'scroll':
        return await executeScrollViaApi(targetTabId, params);

      // Page stability
      case 'wait_for_stable':
        return await executeWaitForStableViaApi(targetTabId, params);

      // Element queries
      case 'get_element':
        return await executeGetElementViaApi(targetTabId, params);

      case 'query_all':
        return await executeQueryAllViaApi(targetTabId, params);

      // Snapshot-based tools (element ID approach)
      case 'snapshot':
        return await executeSnapshotViaApi(targetTabId, params);

      case 'click_by_id':
        return await executeClickByIdViaApi(targetTabId, params, timeout_ms);

      case 'fill_by_id':
        return await executeFillByIdViaApi(targetTabId, params, timeout_ms);

      case 'type_by_id':
        return await executeTypeByIdViaApi(targetTabId, params, timeout_ms);

      // Keyboard control
      case 'key_press':
        return await executeKeyPressViaApi(targetTabId, params);

      // Content extraction
      case 'get_markdown':
        return await executeGetMarkdownViaApi(targetTabId, params);

      // Web fetch (URL to markdown, saved to cache)
      case 'web_fetch':
        return await executeWebFetch(params);

      // Cache tab markdown (tab content to markdown, saved to cache)
      case 'cache_tab_markdown':
        return await executeCacheTabMarkdown(targetTabId, params);

      // Web search
      case 'web_search':
        return await executeWebSearch(params);

      // Ask user a question
      case 'ask_user':
        return await executeAskUser(params);

      // Cache uploaded file (save to disk, return absolute path)
      case 'cache_file':
        return await executeCacheFile(params);

      // Tab management
      case 'list_tabs':
        return await executeListTabs();

      case 'query_tabs':
        return await executeQueryTabs(params);

      // Alias: get_elements → snapshot
      case 'get_elements':
        return await executeSnapshotViaApi(targetTabId, params);

      // Artifact reading (uses existing getArtifact API)
      case 'read_artifact':
        return await executeReadArtifact(params);

      // Artifact editing (uses existing getArtifact + updateArtifact APIs)
      case 'edit_artifact':
        return await executeEditArtifact(params);

      // Canvas rendering (from Code Mode via canvas_render tool)
      case 'canvas_render':
        return await executeCanvasRender(params);

      default:
        return {
          success: false,
          error: { code: -1, message: `Unknown action: ${action}`, recoverable: false },
        };
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
// Artifact Read/Edit (uses existing getArtifact + updateArtifact APIs)
// No browser rebuild required — operates on already-available ContentStore APIs
// =============================================================================

const ARTIFACT_MAX_LINES = 500;

/**
 * Read artifact source code with optional grep/offset/limit
 */
async function executeReadArtifact(params) {
  const id = params.id;
  if (!id) {
    return {
      success: false,
      error: { code: -1, message: 'Missing artifact id', recoverable: false },
    };
  }

  const entry = await browser.nevoflux.getArtifact(id);
  if (!entry.success) {
    return entry; // { success: false, error: ... }
  }

  const content = entry.data?.content || '';
  const allLines = content.split('\n');
  const totalLines = allLines.length;

  // grep mode: find matching lines with context
  if (params.grep) {
    const ctxLines = params.context || 5;
    const needle = params.grep.toLowerCase();
    const matchIndices = [];
    for (let i = 0; i < allLines.length; i++) {
      if (allLines[i].toLowerCase().includes(needle)) {
        matchIndices.push(i);
      }
    }
    if (matchIndices.length === 0) {
      return {
        success: true,
        result: {
          content: '',
          totalLines,
          matches: 0,
          truncated: false,
          title: entry.data?.title,
          type: entry.data?.type,
        },
      };
    }
    const lineSet = new Set();
    for (const idx of matchIndices) {
      for (
        let j = Math.max(0, idx - ctxLines);
        j <= Math.min(allLines.length - 1, idx + ctxLines);
        j++
      ) {
        lineSet.add(j);
      }
    }
    const sortedLines = [...lineSet].sort((a, b) => a - b);
    const sections = [];
    let prev = -2;
    for (const ln of sortedLines) {
      if (ln !== prev + 1 && sections.length > 0) {
        sections.push('...');
      }
      sections.push(`${ln + 1}\t${allLines[ln]}`);
      prev = ln;
    }
    return {
      success: true,
      result: {
        content: sections.join('\n'),
        totalLines,
        matches: matchIndices.length,
        truncated: false,
        title: entry.data?.title,
        type: entry.data?.type,
      },
    };
  }

  // offset/limit mode
  if (params.offset || params.limit) {
    const offset = Math.max(0, (params.offset || 1) - 1);
    const limit = params.limit || ARTIFACT_MAX_LINES;
    const sliced = allLines.slice(offset, offset + limit);
    const numbered = sliced.map((line, i) => `${offset + i + 1}\t${line}`);
    return {
      success: true,
      result: {
        content: numbered.join('\n'),
        totalLines,
        truncated: offset + limit < totalLines,
        title: entry.data?.title,
        type: entry.data?.type,
      },
    };
  }

  // Full read with auto-truncation
  if (totalLines > ARTIFACT_MAX_LINES) {
    const numbered = allLines.slice(0, ARTIFACT_MAX_LINES).map((line, i) => `${i + 1}\t${line}`);
    return {
      success: true,
      result: {
        content:
          numbered.join('\n') +
          `\n\n[Truncated at line ${ARTIFACT_MAX_LINES} of ${totalLines}. Use offset/limit or grep to read more.]`,
        totalLines,
        truncated: true,
        title: entry.data?.title,
        type: entry.data?.type,
      },
    };
  }

  return {
    success: true,
    result: {
      content,
      totalLines,
      truncated: false,
      title: entry.data?.title,
      type: entry.data?.type,
    },
  };
}

/**
 * Edit artifact using search-and-replace pattern
 */
async function executeEditArtifact(params) {
  const { id, old_str, new_str } = params;
  if (!id) {
    return {
      success: false,
      error: { code: -1, message: 'Missing artifact id', recoverable: false },
    };
  }
  if (!old_str) {
    return { success: false, error: { code: -1, message: 'Missing old_str', recoverable: false } };
  }

  const entry = await browser.nevoflux.getArtifact(id);
  if (!entry.success) {
    return entry;
  }

  if (entry.data?.state === 'streaming') {
    return {
      success: false,
      error: {
        code: 12004,
        message: 'Artifact is still generating. Wait for completion.',
        recoverable: true,
      },
    };
  }

  const content = entry.data?.content || '';
  const count = content.split(old_str).length - 1;

  if (count === 0) {
    return {
      success: false,
      error: {
        code: 12005,
        message:
          'old_str not found in artifact. Use browser_read_artifact to verify the current content.',
        recoverable: true,
      },
    };
  }
  if (count > 1) {
    return {
      success: false,
      error: {
        code: 12006,
        message: `old_str matches ${count} locations. Provide more surrounding context to make it unique.`,
        recoverable: true,
      },
    };
  }

  const newContent = content.replace(old_str, new_str);
  const result = await browser.nevoflux.updateArtifact(id, { code: newContent });
  if (!result.success) {
    return result;
  }

  return { success: true, result: { lines: newContent.split('\n').length } };
}

/**
 * Render a multi-file project in the canvas (from Code Mode via canvas_render tool).
 *
 * Creates an artifact with type "project", opens the canvas tab, and broadcasts
 * artifact events to the sidebar for ArtifactCard display.
 *
 * @param {object} params - Tool parameters
 * @param {object} params.files - Object mapping file paths to content strings
 * @param {string} [params.entry] - Entry point file path
 * @param {string} [params.title] - Project title (default: "Generated App")
 * @param {string} [params.artifact_id] - Artifact ID (auto-generated if omitted)
 * @returns {Promise<{success: boolean, result?: object, error?: object}>}
 */
async function executeCanvasRender(params) {
  const files = params?.files;
  if (!files || typeof files !== 'object' || Array.isArray(files)) {
    return {
      success: false,
      error: {
        code: -1,
        message:
          "Missing or invalid 'files' parameter: must be an object mapping file paths to content",
        recoverable: false,
      },
    };
  }

  const title = params?.title || 'Generated App';
  const entry = params?.entry || undefined;
  const id =
    params?.artifact_id || `code-mode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

  // Create artifact in ContentStore
  try {
    await browser.nevoflux.createArtifact({
      id,
      type: 'project',
      title,
      files,
      entry,
      code: '',
      state: 'complete',
      source: 'agent',
      permissions: [],
    });
  } catch (e) {
    console.error('[NevoFlux] canvas_render: Failed to create artifact:', e);
    return {
      success: false,
      error: {
        code: -1,
        message: `Failed to create artifact: ${e.message || e}`,
        recoverable: true,
      },
    };
  }

  // Open canvas tab
  try {
    if (_canvasTabId != null) {
      try {
        await browser.tabs.remove(_canvasTabId);
      } catch {}
      _canvasTabId = null;
    }
    const result = await browser.nevoflux.openCanvasTab(id);
    if (result?.success) {
      if (result.tabId) _canvasTabId = result.tabId;
    } else {
      console.error('[NevoFlux] canvas_render: openCanvasTab failed:', result?.error);
    }
  } catch (e) {
    console.error('[NevoFlux] canvas_render: Failed to open canvas tab:', e);
  }

  // Broadcast to sidebar for ArtifactCard
  broadcastToSidebar({
    type: MessageTypes.ARTIFACT_START,
    payload: { id, content_type: 'project', title },
  });
  broadcastToSidebar({
    type: MessageTypes.ARTIFACT_COMPLETE,
    payload: { id, title },
  });

  return { success: true, result: { artifact_id: id, url: `nevoflux://canvas/${id}` } };
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
  const hasBrowser = typeof browser !== 'undefined';
  const hasNevoflux = hasBrowser && typeof browser.nevoflux !== 'undefined';
  const hasClick = hasNevoflux && typeof browser.nevoflux.click === 'function';

  console.log(
    `[NevoFlux] API check: browser=${hasBrowser}, nevoflux=${hasNevoflux}, click=${hasClick}`
  );

  return hasClick;
}

/**
 * Navigate to a URL via browser.nevoflux.open()
 */
async function executeNavigateViaApi(tabId, params) {
  const { url } = params;
  if (!url) {
    return { success: false, error: { code: -1, message: 'URL required', recoverable: false } };
  }

  try {
    const result = await browser.nevoflux.open(tabId, url);
    if (result.success === false) {
      return result;
    }

    // Wait for page load
    return new Promise((resolve) => {
      const listener = (updatedTabId, changeInfo) => {
        if (updatedTabId === tabId && changeInfo.status === 'complete') {
          browser.tabs.onUpdated.removeListener(listener);
          resolve({ success: true, result: { url } });
        }
      };
      browser.tabs.onUpdated.addListener(listener);

      // Timeout after 30 seconds
      setTimeout(() => {
        browser.tabs.onUpdated.removeListener(listener);
        resolve({
          success: true,
          result: { url, note: 'Navigation started but completion not confirmed' },
        });
      }, 30000);
    });
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Go back in browser history via browser.nevoflux.back()
 */
async function executeGoBackViaApi(tabId) {
  try {
    const result = await browser.nevoflux.back(tabId);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Go forward in browser history via browser.nevoflux.forward()
 */
async function executeGoForwardViaApi(tabId) {
  try {
    const result = await browser.nevoflux.forward(tabId);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Click element via browser.nevoflux.click() - uses trusted mouse events
 * Falls back to content script if API is not available
 */
async function executeClickViaApi(tabId, params) {
  const { selector, button = 'left', click_count = 1 } = params;
  if (!selector) {
    return {
      success: false,
      error: { code: -1, message: 'selector required', recoverable: false },
    };
  }

  // Fallback to content script if API not available
  if (!isNevofluxApiAvailable()) {
    console.log('[NevoFlux] browser.nevoflux not available, using content script');
    return await executeInContentScript(tabId, 'click', params, 30000);
  }

  try {
    const result = await browser.nevoflux.click(tabId, selector, {
      button,
      clickCount: click_count,
    });
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    console.error(
      '[NevoFlux] nevoflux.click failed, falling back to content script:',
      error.message
    );
    return await executeInContentScript(tabId, 'click', params, 30000);
  }
}

/**
 * Type text via browser.nevoflux.type() - uses trusted keyboard events
 * Falls back to content script if API is not available
 */
async function executeTypeViaApi(tabId, params) {
  const { selector, text } = params;
  if (!selector || text === undefined) {
    return {
      success: false,
      error: { code: -1, message: 'selector and text required', recoverable: false },
    };
  }

  if (!isNevofluxApiAvailable()) {
    return await executeInContentScript(tabId, 'type', params, 30000);
  }

  try {
    const result = await browser.nevoflux.type(tabId, selector, text);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    console.error(
      '[NevoFlux] nevoflux.type failed, falling back to content script:',
      error.message
    );
    return await executeInContentScript(tabId, 'type', params, 30000);
  }
}

/**
 * Fill input via browser.nevoflux.fill()
 * Falls back to content script if API is not available
 */
async function executeFillViaApi(tabId, params) {
  const { selector, value } = params;
  if (!selector || value === undefined) {
    return {
      success: false,
      error: { code: -1, message: 'selector and value required', recoverable: false },
    };
  }

  if (!isNevofluxApiAvailable()) {
    return await executeInContentScript(tabId, 'fill', params, 30000);
  }

  try {
    const result = await browser.nevoflux.fill(tabId, selector, value);
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    console.error(
      '[NevoFlux] nevoflux.fill failed, falling back to content script:',
      error.message
    );
    return await executeInContentScript(tabId, 'fill', params, 30000);
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
      // Get full page snapshot — route through executeSnapshotViaApi
      // so refs are stored (needed for click_by_id, fill_by_id, etc.)
      return await executeSnapshotViaApi(tabId, params);
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
    return { success: false, error: { code: -1, message: 'Script required', recoverable: false } };
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
  const { selector, state = 'visible' } = params;
  if (!selector) {
    return {
      success: false,
      error: { code: -1, message: 'selector required', recoverable: false },
    };
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
  const { direction = 'down', amount = 'page' } = params;

  try {
    const result = await browser.nevoflux.scroll(tabId, { direction, amount: String(amount) });
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

async function executeWaitForStableViaApi(tabId, params) {
  const { strategy = 'interaction', maxWait = 3000 } = params;

  try {
    const result = await browser.nevoflux.waitForStable(tabId, { strategy, maxWait });
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
    return {
      success: false,
      error: { code: -1, message: 'selector required', recoverable: false },
    };
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
    return {
      success: false,
      error: { code: -1, message: 'selector required', recoverable: false },
    };
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
// Key: tabId, Value: { refs: {element_id -> ref}, timestamp }
//
// MERGE STRATEGY: New snapshot refs are merged INTO existing refs rather than
// replacing them. This prevents rapid auto-snapshots (e.g. Kimi framework
// appending "Current page state" to every tool response) from invalidating
// element IDs between the AI reading the tree and issuing a click command.
// Each ref entry carries its own `_ts` (timestamp) for per-entry expiry.
const snapshotRefs = new Map();

// Per-entry max age: entries older than this are pruned on access
const REF_ENTRY_MAX_AGE_MS = 60000; // 60 seconds
// Full cleanup threshold
const SNAPSHOT_MAX_AGE_MS = 300000; // 5 minutes

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
    console.log('[NevoFlux] browser.nevoflux not available for snapshot');
    return {
      success: false,
      error: { code: -1, message: 'browser.nevoflux API not available', recoverable: false },
    };
  }

  try {
    // Remove tab_id from params as it's passed separately
    const { tab_id: _tab_id, ...options } = params || {};
    console.log(
      `[NevoFlux] Calling browser.nevoflux.snapshot(${tabId}, ${JSON.stringify(options)})`
    );
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
        const numericId = parseInt(key.replace(/^e/, ''), 10);
        if (!isNaN(numericId)) {
          numericRefs[numericId] = value;
        }
      }
    }

    // Merge new refs into existing refs instead of replacing.
    // New entries overwrite old ones with the same ID; old entries that
    // aren't in the new snapshot are kept (with their original timestamp)
    // until they expire via REF_ENTRY_MAX_AGE_MS.
    const now = Date.now();
    const existing = snapshotRefs.get(tabId);
    const mergedRefs = {};

    // Carry forward non-expired old entries
    if (existing?.refs) {
      for (const [id, ref] of Object.entries(existing.refs)) {
        if (now - (ref._ts || existing.timestamp) < REF_ENTRY_MAX_AGE_MS) {
          mergedRefs[id] = ref;
        }
      }
    }

    // Overlay new entries (always fresher, overwrite old)
    for (const [id, ref] of Object.entries(numericRefs)) {
      mergedRefs[id] = { ...ref, _ts: now };
    }

    snapshotRefs.set(tabId, {
      refs: mergedRefs,
      timestamp: now,
    });

    // Cleanup old snapshots periodically
    cleanupOldSnapshots();

    console.log(
      `[NevoFlux] Stored ${Object.keys(numericRefs).length} new + ${Object.keys(mergedRefs).length - Object.keys(numericRefs).length} carried-over = ${Object.keys(mergedRefs).length} total refs for tab ${tabId}`
    );
    console.log(
      `[NevoFlux] First 5 elements:`,
      Object.entries(numericRefs)
        .slice(0, 5)
        .map(([k, v]) => {
          const sel = v.selectors?.[0]?.value || v.selector || 'no-selector';
          return `${k}: ${sel.substring(0, 50)}`;
        })
        .join('; ')
    );

    return {
      success: true,
      result: {
        tree: result.tree,
        refs: result.refs,
        element_count: Object.keys(result.refs || {}).length,
        stats: result.stats || {
          total: Object.keys(result.refs || {}).length,
          a11y: 0,
          inferred: 0,
          occluded: 0,
          truncated: 0,
        },
        url: result.url || '',
        title: result.title || '',
      },
    };
  } catch (error) {
    console.error('[NevoFlux] browser.nevoflux.snapshot failed:', error.message);
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Get element selector from element ID (stored from snapshot result)
 * @param {number} tabId - Tab ID
 * @param {number} elementId - Element ID from snapshot
 * @param {boolean} getChildren - If true, return array of child selectors (parent first, then children)
 * @returns {string|string[]|null} CSS selector, array of selectors, or null if not found
 */
async function getElementSelector(tabId, elementId, getChildren = false) {
  // Look up from stored snapshot refs
  const tabData = snapshotRefs.get(tabId);

  // Normalize elementId: strip "e" prefix if present (e.g., "e34" -> 34)
  let normalizedId = elementId;
  if (typeof elementId === 'string' && elementId.startsWith('e')) {
    normalizedId = parseInt(elementId.substring(1), 10);
  } else if (typeof elementId === 'string') {
    normalizedId = parseInt(elementId, 10);
  }

  console.log(
    `[NevoFlux] getElementSelector called: tabId=${tabId}, elementId=${elementId} (normalized: ${normalizedId}), getChildren=${getChildren}`
  );
  console.log(`[NevoFlux] snapshotRefs has keys:`, Array.from(snapshotRefs.keys()));

  if (!tabData) {
    console.warn(`[NevoFlux] No snapshot data for tab ${tabId}. Take a snapshot first.`);
    return null;
  }

  console.log(
    `[NevoFlux] tabData.timestamp:`,
    tabData.timestamp,
    `(${(Date.now() - tabData.timestamp) / 1000}s ago)`
  );
  console.log(`[NevoFlux] tabData.refs has ${Object.keys(tabData.refs).length} elements`);
  console.log(
    `[NevoFlux] Available element IDs (first 20):`,
    Object.keys(tabData.refs).slice(0, 20).join(', ')
  );

  const elementRef = tabData.refs[normalizedId];

  if (!elementRef) {
    console.warn(
      `[NevoFlux] Element ID ${elementId} (normalized: ${normalizedId}) not found in snapshot refs (merged).`
    );
    // Log nearby IDs to help debug
    const allIds = Object.keys(tabData.refs)
      .map(Number)
      .sort((a, b) => a - b);
    const nearbyIds = allIds.filter((id) => Math.abs(id - normalizedId) <= 5);
    console.warn(`[NevoFlux] Nearby IDs: ${nearbyIds.join(', ')}`);
    return null;
  }

  console.log(
    `[NevoFlux] Found element ${elementId} (normalized: ${normalizedId}):`,
    JSON.stringify(elementRef).substring(0, 300)
  );

  // Extract best CSS selector from the selectors array (new format)
  // Also supports legacy single-selector format for backward compatibility
  function getBestCssSelector(ref) {
    // New format: selectors array
    if (ref.selectors && Array.isArray(ref.selectors)) {
      // Prefer CSS selectors in priority order (skip a11y: locators for CSS-based operations)
      for (const s of ref.selectors) {
        if (s.type === 'css') return s.value;
      }
      // Fallback: return first selector value regardless of type
      return ref.selectors[0]?.value || null;
    }
    // Legacy format: single selector string
    return ref.selector || null;
  }

  // If getChildren is true, return array with parent and all direct children
  if (getChildren) {
    const parentSelector = getBestCssSelector(elementRef);
    const allRefs = Object.entries(tabData.refs);

    // Find direct children by checking CSS path relationship
    const childRefs = allRefs.filter(([_id, ref]) => {
      const sel = getBestCssSelector(ref);
      if (!sel || sel === parentSelector) return false;
      if (!sel.startsWith(parentSelector)) return false;
      const afterParent = sel.substring(parentSelector.length);
      return afterParent.startsWith('>') && !afterParent.substring(1).includes('>');
    });

    childRefs.sort((a, b) => Number(a[0]) - Number(b[0]));

    const selectors = [];
    for (const [_id, ref] of childRefs) {
      selectors.push(getBestCssSelector(ref));
    }
    selectors.push(parentSelector);

    console.log(
      `[NevoFlux] Returning ${selectors.length} selectors (${childRefs.length} children + 1 parent)`
    );
    return selectors;
  }

  return getBestCssSelector(elementRef);
}

/**
 * Normalize an element ID to its numeric form.
 * Handles "e34" -> 34, "34" -> 34, and passthrough for numbers.
 */
function normalizeElementId(elementId) {
  if (typeof elementId === 'string' && elementId.startsWith('e')) {
    return parseInt(elementId.substring(1), 10);
  }
  if (typeof elementId === 'string') {
    return parseInt(elementId, 10);
  }
  return elementId;
}

/**
 * Try a coordinate-based click fallback using stored rect from snapshotRefs.
 * Returns a success result object or null if fallback not possible / not effective.
 */
async function tryCoordinateClickFallback(tabId, element_id) {
  const normalizedId = normalizeElementId(element_id);
  const tabData = snapshotRefs.get(tabId);
  const elemRef = tabData?.refs?.[normalizedId];
  const elemRect = elemRef?.rect;

  if (!elemRef) {
    console.log(
      `[NevoFlux] Coordinate fallback: element ${element_id} (normalized: ${normalizedId}) not in refs`
    );
    return null;
  }
  if (!elemRect) {
    console.log(
      `[NevoFlux] Coordinate fallback: element ${element_id} has no rect. Ref keys: ${Object.keys(elemRef).join(',')}`
    );
    return null;
  }
  if (elemRect.width <= 0 || elemRect.height <= 0) {
    console.log(
      `[NevoFlux] Coordinate fallback: element ${element_id} has zero-size rect: ${JSON.stringify(elemRect)}`
    );
    return null;
  }

  const centerX = elemRect.x + elemRect.width / 2;
  const centerY = elemRect.y + elemRect.height / 2;
  console.log(
    `[NevoFlux] Trying coordinate click at (${centerX}, ${centerY}) for element ${element_id}, rect=${JSON.stringify(elemRect)}`
  );

  try {
    const coordResult = await browser.nevoflux.clickAtCoordinates(tabId, centerX, centerY);
    if (coordResult.success !== false) {
      console.log(
        `[NevoFlux] Coordinate click for element ${element_id}: effective=${coordResult.effective}`
      );
      return {
        success: true,
        result: {
          element_id,
          clicked: true,
          method: 'coordinate_click',
          ...coordResult,
        },
      };
    }
  } catch (coordErr) {
    console.log('[NevoFlux] Coordinate click fallback failed:', coordErr.message);
  }
  return null;
}

/**
 * Click element by ID via browser.nevoflux.click() - uses trusted mouse events
 * Falls back to content script if API is not available or fails
 */
async function executeClickByIdViaApi(tabId, params, timeout_ms) {
  const { element_id } = params;

  if (!element_id) {
    return {
      success: false,
      error: { code: -1, message: 'element_id required', recoverable: false },
    };
  }

  // Fallback to content script if API not available
  if (!isNevofluxApiAvailable()) {
    console.log('[NevoFlux] browser.nevoflux not available, using content script for click_by_id');
    return await executeInContentScript(tabId, 'click_by_id', params, timeout_ms);
  }

  try {
    // Get selectors: parent element + all direct children
    const selectors = await getElementSelector(tabId, element_id, true);

    if (!selectors || selectors.length === 0) {
      // No selectors found — try coordinate click using stored rect from snapshot
      const coordFallback = await tryCoordinateClickFallback(tabId, element_id);
      if (coordFallback) return coordFallback;

      return {
        success: false,
        error: {
          code: -1,
          message: `Element ID ${element_id} not found. Take a new snapshot first.`,
          recoverable: true,
        },
      };
    }

    // Handle both single selector (string) and multiple selectors (array)
    // Filter out null/empty selectors (e.g. ?cursor/?tag elements with no CSS selector)
    const rawList = Array.isArray(selectors) ? selectors : [selectors];
    const selectorList = rawList.filter((s) => s != null && s !== '');

    // If all selectors were null/empty, go straight to coordinate fallback
    if (selectorList.length === 0) {
      console.log(
        `[NevoFlux] Element ${element_id} found in refs but has no CSS selectors, trying coordinate click`
      );
      const coordFallback = await tryCoordinateClickFallback(tabId, element_id);
      if (coordFallback) return coordFallback;

      return {
        success: false,
        error: {
          code: -1,
          message: `Element ID ${element_id} has no CSS selector and coordinate click failed.`,
          recoverable: true,
        },
      };
    }

    console.log(
      `[NevoFlux] Trying to click element ${element_id}, ${selectorList.length} candidate(s)`
    );

    // Try each selector until one has detectable effect
    let lastError = null;
    let lastResult = null;
    for (let i = 0; i < selectorList.length; i++) {
      const selector = selectorList[i];
      console.log(
        `[NevoFlux] Attempt ${i + 1}/${selectorList.length}: clicking '${selector.substring(0, 100)}...'`
      );

      try {
        const result = await browser.nevoflux.click(tabId, selector);

        if (result.success === false) {
          lastError = result.error || { message: 'Click returned success=false' };
          console.log(`[NevoFlux] Attempt ${i + 1} failed:`, lastError.message || lastError);
          continue;
        }

        // Check if click had detectable effect (DOM change, network request, or element removed)
        const effective = result.effective === true;
        console.log(
          `[NevoFlux] Attempt ${i + 1} - effective: ${effective}, domChanged: ${result.domChanged}, networkRequest: ${result.networkRequestMade}, elementRemoved: ${result.elementRemoved}`
        );

        if (effective) {
          console.log(`[NevoFlux] Click effective on attempt ${i + 1}`);
          return {
            success: true,
            result: {
              element_id,
              selector,
              clicked: true,
              method: 'nevoflux_api',
              attempt: i + 1,
              ...result,
            },
          };
        }

        // Click executed but no detectable effect - try next selector
        lastResult = result;
        console.log(`[NevoFlux] Attempt ${i + 1} no effect, trying next...`);
      } catch (clickError) {
        lastError = { message: clickError.message || String(clickError) };
        console.log(`[NevoFlux] Attempt ${i + 1} threw error:`, lastError.message);
      }
    }

    // All attempts had no detectable effect
    // Return last result if any click was executed (might still have worked, just not detected)
    if (lastResult) {
      // For ?cursor elements (text with cursor:pointer but no own handler),
      // try coordinate click — physical events bubble to ancestor handlers.
      // Guard: only ?cursor. Other signals mean the element IS interactive,
      // so "not effective" is a detection miss, not a real failure.
      // Double-clicking would break toggles/modals.
      const normalizedId = normalizeElementId(element_id);
      const curTabData = snapshotRefs.get(tabId);
      const elementRef = curTabData?.refs?.[normalizedId];
      if (elementRef?.signal === 'cursor') {
        console.log(
          `[NevoFlux] ?cursor element ${element_id} click ineffective, trying coordinate fallback`
        );
        const coordFallback = await tryCoordinateClickFallback(tabId, element_id);
        if (coordFallback?.result?.effective) return coordFallback;
      }

      console.log(
        `[NevoFlux] All ${selectorList.length} attempts had no detectable effect, returning last result`
      );
      return {
        success: true,
        result: {
          element_id,
          selector: selectorList[selectorList.length - 1],
          clicked: true,
          method: 'nevoflux_api',
          effective: false,
          ...lastResult,
        },
      };
    }

    // All selector-based attempts truly failed — try coordinate-based click
    // This handles cross-origin iframe elements where querySelector returns null
    const coordFallback = await tryCoordinateClickFallback(tabId, element_id);
    if (coordFallback) return coordFallback;

    // All attempts truly failed
    console.error(
      `[NevoFlux] All ${selectorList.length} click attempts + coordinate fallback failed`
    );
    return {
      success: false,
      error: {
        code: -1,
        message: `All click attempts failed. Last error: ${lastError?.message || 'unknown'}`,
        recoverable: true,
      },
    };
  } catch (error) {
    console.error('[NevoFlux] nevoflux.click failed:', error.message);
    // Fallback to content script
    return await executeInContentScript(tabId, 'click_by_id', params, timeout_ms);
  }
}

/**
 * Fill element by ID via browser.nevoflux.fill() + keyPress for Enter
 * Falls back to content script if API is not available or fails
 */
async function executeFillByIdViaApi(tabId, params, timeout_ms) {
  const { element_id, value, press_enter = false } = params;

  console.log(
    `[NevoFlux] executeFillByIdViaApi: element_id=${element_id}, value=${value?.substring(0, 20)}, press_enter=${press_enter}`
  );

  if (!element_id || value === undefined) {
    return {
      success: false,
      error: { code: -1, message: 'element_id and value required', recoverable: false },
    };
  }

  // Fallback to content script if API not available
  if (!isNevofluxApiAvailable()) {
    console.log('[NevoFlux] browser.nevoflux not available, using content script for fill_by_id');
    return await executeInContentScript(tabId, 'fill_by_id', params, timeout_ms);
  }

  try {
    // Get selector from element ID
    console.log(`[NevoFlux] Getting selector for element_id=${element_id}`);
    const selector = await getElementSelector(tabId, element_id);
    console.log(`[NevoFlux] Got selector: ${selector}`);

    if (!selector) {
      return {
        success: false,
        error: {
          code: -1,
          message: `Element ID ${element_id} not found. Take a new snapshot first.`,
          recoverable: true,
        },
      };
    }

    console.log(
      `[NevoFlux] Filling element ${element_id} via browser.nevoflux.fill('${selector}', '${value.substring(0, 20)}...')`
    );

    // Click to focus first
    console.log(`[NevoFlux] Step 1: Clicking to focus...`);
    const clickResult = await browser.nevoflux.click(tabId, selector);
    console.log(`[NevoFlux] Click result:`, clickResult);
    await new Promise((r) => setTimeout(r, 100));

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
      await new Promise((r) => setTimeout(r, 100));
      // Focus again in case it was lost
      console.log(`[NevoFlux] Step 4: Re-focusing...`);
      const focusResult = await browser.nevoflux.focus(tabId, selector);
      console.log(`[NevoFlux] Focus result:`, focusResult);
      await new Promise((r) => setTimeout(r, 50));

      console.log(`[NevoFlux] Step 5: Pressing Enter...`);
      const enterResult = await browser.nevoflux.keyPress(tabId, 'Enter');
      console.log(`[NevoFlux] Enter result:`, enterResult);
    }

    return {
      success: true,
      result: {
        element_id,
        selector,
        filled: value,
        enter_pressed: press_enter,
        method: 'nevoflux_api',
      },
    };
  } catch (error) {
    console.error('[NevoFlux] nevoflux.fill failed at some step:', error.message, error.stack);
    // Fallback to content script
    return await executeInContentScript(tabId, 'fill_by_id', params, timeout_ms);
  }
}

/**
 * Type text into element by ID via browser.nevoflux.type() - uses trusted keyboard events
 * Falls back to content script if API is not available or fails
 */
async function executeTypeByIdViaApi(tabId, params, timeout_ms) {
  const { element_id, text, press_enter = false } = params;

  if (!element_id || text === undefined) {
    return {
      success: false,
      error: { code: -1, message: 'element_id and text required', recoverable: false },
    };
  }

  // Fallback to content script if API not available
  if (!isNevofluxApiAvailable()) {
    console.log('[NevoFlux] browser.nevoflux not available, using content script for type_by_id');
    return await executeInContentScript(tabId, 'type_by_id', params, timeout_ms);
  }

  try {
    // Get selector from element ID
    const selector = await getElementSelector(tabId, element_id);

    if (!selector) {
      return {
        success: false,
        error: {
          code: -1,
          message: `Element ID ${element_id} not found. Take a new snapshot first.`,
          recoverable: true,
        },
      };
    }

    console.log(
      `[NevoFlux] Typing into element ${element_id} via browser.nevoflux.type('${selector}', '${text.substring(0, 20)}...')`
    );

    // Click to focus first
    await browser.nevoflux.click(tabId, selector);
    await new Promise((r) => setTimeout(r, 100));

    // Type text character by character - uses trusted keyboard events
    const result = await browser.nevoflux.type(tabId, selector, text);

    if (result.success === false) {
      return result;
    }

    // Press Enter if requested
    if (press_enter) {
      await new Promise((r) => setTimeout(r, 100));
      // Focus again in case it was lost
      await browser.nevoflux.focus(tabId, selector);
      await new Promise((r) => setTimeout(r, 50));
      await browser.nevoflux.keyPress(tabId, 'Enter');
    }

    return {
      success: true,
      result: {
        element_id,
        selector,
        typed: text,
        enter_pressed: press_enter,
        method: 'nevoflux_api',
      },
    };
  } catch (error) {
    console.error('[NevoFlux] nevoflux.type failed:', error.message);
    // Fallback to content script
    return await executeInContentScript(tabId, 'type_by_id', params, timeout_ms);
  }
}

/**
 * Press key via browser.nevoflux.keyPress() - uses trusted keyboard events
 */
async function executeKeyPressViaApi(tabId, params) {
  const { key, modifiers = [] } = params;
  if (!key) {
    return { success: false, error: { code: -1, message: 'key required', recoverable: false } };
  }

  try {
    const result = await browser.nevoflux.keyPress(tabId, key, { modifiers });
    return result.success !== undefined ? result : { success: true, result };
  } catch (error) {
    // If the error is "Actor destroyed", it means the page navigated after the key press
    // This is expected for Enter key on forms, so treat it as success
    if (error.message && error.message.includes('destroyed')) {
      console.log(
        `[NevoFlux] keyPress '${key}' triggered navigation (Actor destroyed) - treating as success`
      );
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

    // Check for error response
    if (result.success === false) {
      return result;
    }

    // Wrap successful result in the expected format { success, result }
    // The raw response has { success, markdown, title, url } which needs
    // to be wrapped so the sidebar can access it via result.markdown
    return { success: true, result };
  } catch (error) {
    return { success: false, error: { code: -1, message: error.message, recoverable: true } };
  }
}

/**
 * Cache tab markdown: Get tab content as markdown and save to cache file
 * Returns the file path for the agent to read
 */
async function executeCacheTabMarkdown(tabId, params) {
  const { max_length = 100000 } = params;

  console.log('[NevoFlux] CacheTabMarkdown: Getting markdown for tab', tabId);

  // Get tab info for URL
  let tabUrl = '';
  let tabTitle = '';
  try {
    const tab = await browser.tabs.get(tabId);
    tabUrl = tab.url || '';
    tabTitle = tab.title || '';
  } catch (e) {
    console.warn('[NevoFlux] CacheTabMarkdown: Failed to get tab info:', e.message);
  }

  // Get markdown from tab via browser.nevoflux.getMarkdown()
  let markdown;
  try {
    const result = await browser.nevoflux.getMarkdown(tabId, params);

    if (result.success === false) {
      return {
        success: false,
        error: result.error || { code: -1, message: 'getMarkdown failed', recoverable: true },
      };
    }

    // Extract markdown from result
    if (typeof result === 'string') {
      markdown = result;
    } else if (result.markdown) {
      markdown = result.markdown;
    } else if (result.result && typeof result.result === 'string') {
      markdown = result.result;
    } else if (result.result && result.result.markdown) {
      markdown = result.result.markdown;
    } else {
      markdown = JSON.stringify(result);
    }

    // Use title from result if available
    if (result.title) tabTitle = result.title;
    if (result.result?.title) tabTitle = result.result.title;
  } catch (error) {
    return {
      success: false,
      error: { code: -1, message: `getMarkdown failed: ${error.message}`, recoverable: true },
    };
  }

  if (!markdown) {
    return {
      success: false,
      error: { code: -1, message: 'No markdown content extracted', recoverable: true },
    };
  }

  // Truncate if needed
  if (markdown.length > max_length) {
    markdown = markdown.substring(0, max_length) + '\n\n[Content truncated...]';
  }

  // Generate cache file path using tab URL or ID
  const cacheKey = tabUrl || `tab_${tabId}`;
  const urlHash = await hashString(cacheKey);
  const cacheDir = await getCacheDirectory();
  const cacheFilePath = `${cacheDir}/${urlHash}.md`;

  console.log(
    '[NevoFlux] CacheTabMarkdown: Success, markdown length:',
    markdown.length,
    'path:',
    cacheFilePath
  );

  return {
    success: true,
    result: {
      file_path: cacheFilePath,
      url: tabUrl,
      title: tabTitle,
      tab_id: tabId,
      content_length: markdown.length,
      // Include markdown directly for agent to save
      _markdown: markdown,
    },
  };
}

// =============================================================================
// Cache File Implementation
// =============================================================================

/**
 * Cache uploaded file to disk and return the absolute path
 * Params: { name: string, content: string (base64), mime_type: string }
 * Returns: { file_path: string, name: string, size: number }
 */
async function executeCacheFile(params) {
  const { name, content, mime_type } = params;

  if (!name || !content) {
    return {
      success: false,
      error: { code: -1, message: 'Missing required params: name and content', recoverable: false },
    };
  }

  console.log('[NevoFlux] CacheFile: Caching file', name, 'mime:', mime_type);

  // Generate cache file path
  const timestamp = Date.now();
  const safeFileName = name.replace(/[^a-zA-Z0-9._-]/g, '_');
  const cacheDir = await getCacheDirectory();
  const cacheFilePath = `${cacheDir}/upload_${timestamp}_${safeFileName}`;

  // Decode base64 content
  let decodedContent;
  try {
    // For text files, decode base64 to string
    if (mime_type && mime_type.startsWith('text/')) {
      decodedContent = atob(content);
    } else {
      // For binary files, keep as base64 - agent will handle decoding
      decodedContent = content;
    }
  } catch (e) {
    return {
      success: false,
      error: { code: -1, message: `Failed to decode content: ${e.message}`, recoverable: false },
    };
  }

  console.log(
    '[NevoFlux] CacheFile: Success, path:',
    cacheFilePath,
    'size:',
    decodedContent.length
  );

  return {
    success: true,
    result: {
      file_path: cacheFilePath,
      name: name,
      size: decodedContent.length,
      mime_type: mime_type,
      // Include content for agent to save to disk
      _content: decodedContent,
      _is_base64: !mime_type || !mime_type.startsWith('text/'),
    },
  };
}

// =============================================================================
// Tab Management Implementation
// =============================================================================

/**
 * List all open tabs via browser.nevoflux.listTabs()
 */
async function executeListTabs() {
  try {
    // Try privileged API first
    if (isNevofluxApiAvailable()) {
      const tabs = await browser.nevoflux.listTabs();
      if (tabs && tabs.length > 0) {
        return { success: true, result: { tabs } };
      }
    }
    // Fallback to standard WebExtension tabs API
    const allTabs = await browser.tabs.query({});
    const tabs = allTabs.map((t) => ({
      id: t.id,
      url: t.url || '',
      title: t.title || '',
      active: t.active,
      index: t.index,
      windowId: t.windowId,
      status: t.status || 'complete',
    }));
    return { success: true, result: { tabs } };
  } catch (error) {
    return {
      success: false,
      error: { code: -1, message: error.message || String(error), recoverable: true },
    };
  }
}

/**
 * Query tabs with optional filters (url, title, active) via browser.nevoflux.queryTabs()
 */
async function executeQueryTabs(params) {
  try {
    // Try privileged API first
    if (isNevofluxApiAvailable()) {
      const tabs = await browser.nevoflux.queryTabs(params || {});
      if (tabs && tabs.length > 0) {
        return { success: true, result: { tabs } };
      }
    }
    // Fallback to standard WebExtension tabs API
    const query = {};
    if (params?.active !== undefined) query.active = params.active;
    if (params?.windowId !== undefined) query.windowId = params.windowId;
    if (params?.url) query.url = params.url;
    if (params?.title) query.title = params.title;
    const allTabs = await browser.tabs.query(query);
    const tabs = allTabs.map((t) => ({
      id: t.id,
      url: t.url || '',
      title: t.title || '',
      active: t.active,
      index: t.index,
      windowId: t.windowId,
      status: t.status || 'complete',
    }));
    return { success: true, result: { tabs } };
  } catch (error) {
    return {
      success: false,
      error: { code: -1, message: error.message || String(error), recoverable: true },
    };
  }
}

// =============================================================================
// Web Search Implementation
// =============================================================================

/**
 * Execute web search using DuckDuckGo HTML search
 * Returns search results without requiring an API key
 */
async function executeWebSearch(params) {
  const { query, max_results = 10, timeout_ms = 30000 } = params;

  // Validate query
  if (!query || typeof query !== 'string' || query.trim().length === 0) {
    return {
      success: false,
      error: { code: 7001, message: 'Search query is required', recoverable: false },
    };
  }

  const searchQuery = query.trim();
  console.log('[NevoFlux] WebSearch: Searching for:', searchQuery);

  // Use DuckDuckGo HTML search
  const searchUrl = `https://html.duckduckgo.com/html/?q=${encodeURIComponent(searchQuery)}`;

  let response;
  try {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeout_ms);

    response = await fetch(searchUrl, {
      signal: controller.signal,
      headers: {
        'User-Agent':
          'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
        Accept: 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
        'Accept-Language': 'en-US,en;q=0.9',
      },
    });

    clearTimeout(timeoutId);
  } catch (e) {
    const message = e.name === 'AbortError' ? 'Search timed out' : e.message;
    return {
      success: false,
      error: { code: 7002, message: `Search failed: ${message}`, recoverable: true },
    };
  }

  if (!response.ok) {
    return {
      success: false,
      error: {
        code: 7002,
        message: `HTTP ${response.status}: ${response.statusText}`,
        recoverable: true,
      },
    };
  }

  // Parse HTML response
  let html;
  try {
    html = await response.text();
  } catch (e) {
    return {
      success: false,
      error: { code: 7002, message: `Failed to read response: ${e.message}`, recoverable: true },
    };
  }

  // Parse search results from DuckDuckGo HTML
  const results = parseDuckDuckGoResults(html, max_results);

  console.log('[NevoFlux] WebSearch: Found', results.length, 'results');

  return {
    success: true,
    result: {
      results: results,
      query: searchQuery,
      total_results: results.length,
    },
  };
}

/**
 * Parse DuckDuckGo HTML search results
 * @param {string} html - HTML content
 * @param {number} maxResults - Maximum number of results to return
 * @returns {Array<{title: string, url: string, snippet: string}>}
 */
function parseDuckDuckGoResults(html, maxResults) {
  const results = [];

  try {
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');

    // DuckDuckGo HTML results are in div.result elements
    const resultElements = doc.querySelectorAll('.result');

    for (const el of resultElements) {
      if (results.length >= maxResults) break;

      // Skip ads and non-result elements
      if (el.classList.contains('result--ad')) continue;

      // Extract title and URL from the result__a link
      const titleLink = el.querySelector('.result__a');
      if (!titleLink) continue;

      const title = titleLink.textContent?.trim() || '';
      let url = titleLink.getAttribute('href') || '';

      // DuckDuckGo wraps URLs in a redirect, extract the actual URL
      if (url.startsWith('//duckduckgo.com/l/?')) {
        const urlParams = new URLSearchParams(url.split('?')[1] || '');
        url = urlParams.get('uddg') || url;
      }

      // Decode URL if needed
      try {
        url = decodeURIComponent(url);
      } catch (e) {
        // Keep original URL if decoding fails
      }

      // Skip if no valid URL
      if (!url || url.startsWith('//duckduckgo.com')) continue;

      // Extract snippet from result__snippet
      const snippetEl = el.querySelector('.result__snippet');
      const snippet = snippetEl?.textContent?.trim() || '';

      results.push({
        title,
        url,
        snippet,
      });
    }
  } catch (e) {
    console.error('[NevoFlux] WebSearch: Failed to parse results:', e.message);
  }

  return results;
}

// =============================================================================
// Ask User Implementation
// =============================================================================

// Pending AskUser requests map: requestId -> {resolve, reject, timeoutId}
const pendingAskUserRequests = new Map();

/**
 * Execute ask user: Show question to user and wait for response
 */
async function executeAskUser(params) {
  const { question, options = [], allow_custom = true, timeout_ms = 60000 } = params;

  // Validate question
  if (!question || typeof question !== 'string' || question.trim().length === 0) {
    return {
      success: false,
      error: { code: 8001, message: 'Question is required', recoverable: false },
    };
  }

  const requestId = `ask_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

  console.log('[NevoFlux] AskUser: Sending question to sidebar:', question);

  // Create promise that will be resolved when user responds
  return new Promise((resolve) => {
    // Set timeout
    const timeoutId = setTimeout(() => {
      if (pendingAskUserRequests.has(requestId)) {
        pendingAskUserRequests.delete(requestId);
        console.log('[NevoFlux] AskUser: Timed out waiting for user response');
        resolve({
          success: false,
          error: { code: 8001, message: 'User interaction timed out', recoverable: true },
        });
      }
    }, timeout_ms);

    // Store pending request
    pendingAskUserRequests.set(requestId, {
      resolve,
      timeoutId,
    });

    // Send request to sidebar
    broadcastToSidebar({
      type: MessageTypes.ASK_USER_REQUEST,
      payload: {
        request_id: requestId,
        question: question.trim(),
        options: options,
        allow_custom: allow_custom,
        timeout_ms: timeout_ms,
      },
    });
  });
}

/**
 * Handle ask user response from sidebar
 */
function handleAskUserResponse(payload) {
  const { request_id, answer, is_custom, selected_index, cancelled } = payload;

  const pending = pendingAskUserRequests.get(request_id);
  if (!pending) {
    console.warn('[NevoFlux] AskUser: No pending request found for', request_id);
    return;
  }

  // Clear timeout and remove from pending
  clearTimeout(pending.timeoutId);
  pendingAskUserRequests.delete(request_id);

  if (cancelled) {
    console.log('[NevoFlux] AskUser: User cancelled');
    pending.resolve({
      success: false,
      error: { code: 8001, message: 'User cancelled the interaction', recoverable: true },
    });
    return;
  }

  console.log('[NevoFlux] AskUser: Received response:', answer);
  pending.resolve({
    success: true,
    result: {
      answer: answer,
      is_custom: is_custom || false,
      selected_index: selected_index !== undefined ? selected_index : -1,
    },
  });
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
      type: 'browser_tool_action',
      action,
      params,
    });
  };

  // Helper function to inject content script
  const injectContentScript = async () => {
    console.log('[NevoFlux] Injecting content script into tab', tabId);
    await browser.scripting.executeScript({
      target: { tabId },
      files: ['content/content.js'],
    });
    // Small delay to let the script initialize
    await new Promise((r) => setTimeout(r, 100));
  };

  // Refactor: avoid async in Promise constructor to ensure proper timeout cleanup
  let timeoutId = null;
  let _resolved = false;

  try {
    // First try: send message to existing content script
    const response = await Promise.race([
      sendActionMessage(),
      new Promise(
        (_, reject) =>
          (timeoutId = setTimeout(
            () => reject(new Error(`Action timed out after ${timeout_ms}ms`)),
            timeout_ms
          ))
      ),
    ]);

    _resolved = true;
    if (timeoutId) clearTimeout(timeoutId);
    if (response && response.success !== undefined) {
      return response;
    } else {
      return { success: true, result: response };
    }
  } catch (error) {
    // Content script not loaded, try injecting it
    console.warn('[NevoFlux] Content script not responding, injecting:', error.message);

    try {
      // Calculate remaining time after first attempt
      const remainingMs = Math.max(0, timeout_ms - 500);

      await injectContentScript();

      // Retry the action after injection with remaining time
      const response = await Promise.race([
        sendActionMessage(),
        new Promise(
          (_, reject) =>
            (timeoutId = setTimeout(
              () => reject(new Error(`Action timed out after ${timeout_ms}ms`)),
              remainingMs
            ))
        ),
      ]);

      _resolved = true;
      if (timeoutId) clearTimeout(timeoutId);
      if (response && response.success !== undefined) {
        return response;
      } else {
        return { success: true, result: response };
      }
    } catch (injectError) {
      if (timeoutId) clearTimeout(timeoutId);
      console.error('[NevoFlux] Failed to execute in content script:', injectError.message);
      return {
        success: false,
        error: {
          code: -1,
          message: `Content script error: ${injectError.message}`,
          recoverable: true,
        },
      };
    }
  }
}

// =============================================================================
// Web Fetch Implementation
// Fetches URL, converts to markdown, saves to cache
// =============================================================================

/**
 * Execute web fetch: URL → fetch → markdown → cache file → return path
 */
async function executeWebFetch(params) {
  const {
    url,
    timeout_ms = 30000,
    include_images = false,
    max_length = 100000,
    force_refresh = false,
  } = params;

  // Validate URL
  if (!url) {
    return {
      success: false,
      error: { code: 6002, message: 'URL is required', recoverable: false },
    };
  }

  let parsedUrl;
  try {
    parsedUrl = new URL(url);
    if (!['http:', 'https:'].includes(parsedUrl.protocol)) {
      throw new Error('Only HTTP/HTTPS URLs are supported');
    }
  } catch (e) {
    return {
      success: false,
      error: { code: 6002, message: `Invalid URL: ${e.message}`, recoverable: false },
    };
  }

  // Generate cache file path
  const urlHash = await hashString(url);
  const cacheDir = await getCacheDirectory();
  const cacheFilePath = `${cacheDir}/${urlHash}.md`;
  const metaFilePath = `${cacheDir}/${urlHash}.meta`;

  // Check cache (unless force_refresh)
  if (!force_refresh) {
    try {
      const cached = await checkCache(cacheFilePath, metaFilePath);
      if (cached) {
        console.log('[NevoFlux] WebFetch: Using cached content for', url);
        return {
          success: true,
          result: {
            file_path: cacheFilePath,
            url: url,
            title: cached.title || '',
            content_length: cached.content_length || 0,
            cached: true,
          },
        };
      }
    } catch (e) {
      console.log('[NevoFlux] WebFetch: Cache check failed, will fetch fresh:', e.message);
    }
  }

  // Fetch the URL
  console.log('[NevoFlux] WebFetch: Fetching', url);
  let response;
  try {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeout_ms);

    response = await fetch(url, {
      signal: controller.signal,
      headers: {
        'User-Agent': 'Mozilla/5.0 (compatible; NevoFlux/1.0; +https://nevoflux.com)',
        Accept: 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
      },
    });

    clearTimeout(timeoutId);
  } catch (e) {
    const message = e.name === 'AbortError' ? 'Request timed out' : e.message;
    return {
      success: false,
      error: { code: 6001, message: `Fetch failed: ${message}`, recoverable: true },
    };
  }

  if (!response.ok) {
    return {
      success: false,
      error: {
        code: 6001,
        message: `HTTP ${response.status}: ${response.statusText}`,
        recoverable: true,
      },
    };
  }

  // Check content type
  const contentType = response.headers.get('content-type') || '';
  if (!contentType.includes('text/html') && !contentType.includes('application/xhtml')) {
    return {
      success: false,
      error: {
        code: 6003,
        message: `Content type not supported: ${contentType}`,
        recoverable: false,
      },
    };
  }

  // Get HTML content
  let html;
  try {
    html = await response.text();
  } catch (e) {
    return {
      success: false,
      error: { code: 6001, message: `Failed to read response: ${e.message}`, recoverable: true },
    };
  }

  // Check size
  if (html.length > max_length * 10) {
    // Allow 10x for HTML since markdown will be smaller
    return {
      success: false,
      error: { code: 6004, message: `Content too large: ${html.length} bytes`, recoverable: false },
    };
  }

  // Convert HTML to Markdown
  let markdown, title;
  try {
    const result = htmlToMarkdown(html, { includeImages: include_images, maxLength: max_length });
    markdown = result.markdown;
    title = result.title;
  } catch (e) {
    return {
      success: false,
      error: {
        code: 6001,
        message: `Markdown conversion failed: ${e.message}`,
        recoverable: false,
      },
    };
  }

  // Truncate if needed
  if (markdown.length > max_length) {
    markdown = markdown.substring(0, max_length) + '\n\n[Content truncated...]';
  }

  // Save to cache via native messaging (agent will write the file)
  try {
    await saveToCacheViaAgent(cacheFilePath, metaFilePath, markdown, {
      url,
      title,
      content_length: markdown.length,
    });
  } catch (e) {
    console.error('[NevoFlux] WebFetch: Failed to save cache:', e.message);
    // Continue anyway, just won't be cached
  }

  console.log('[NevoFlux] WebFetch: Success, markdown length:', markdown.length);

  return {
    success: true,
    result: {
      file_path: cacheFilePath,
      url: url,
      title: title || '',
      content_length: markdown.length,
      cached: false,
      // Include markdown directly for agent to save
      _markdown: markdown,
    },
  };
}

/**
 * Hash a string using SHA-256
 */
async function hashString(str) {
  const encoder = new TextEncoder();
  const data = encoder.encode(str);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Get cache directory path
 */
async function getCacheDirectory() {
  // Use a standard cache location - agent will handle actual file operations
  const home = await getHomeDirectory();
  return `${home}/.cache/nevoflux/web_fetch`;
}

/**
 * Get home directory (platform-aware)
 */
async function getHomeDirectory() {
  // This will be determined by the agent side
  // For now, return a placeholder that agent will resolve
  if (navigator.platform.startsWith('Win')) {
    return '%USERPROFILE%';
  }
  return '~';
}

/**
 * Check if valid cache exists
 */
async function checkCache(_cacheFilePath, _metaFilePath) {
  // Cache checking is done via native agent
  // For now, return null to always fetch fresh
  // TODO: Implement cache checking via native messaging
  return null;
}

/**
 * Save content to cache via native agent
 */
async function saveToCacheViaAgent(_cacheFilePath, _metaFilePath, _markdown, _meta) {
  // The agent will handle the actual file writing
  // The _markdown field in the result will be used by the agent
  return true;
}

/**
 * Convert HTML to Markdown
 * Simplified implementation for background script context
 */
function htmlToMarkdown(html, options = {}) {
  const { includeImages = false, maxLength: _maxLength = 100000 } = options;

  // Parse HTML
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');

  // Get title
  const title = doc.title || '';

  // Find main content
  const mainContent = findMainContent(doc);

  // Convert to markdown
  const markdown = convertElementToMarkdown(mainContent, { includeImages });

  return { markdown: cleanMarkdown(markdown), title };
}

/**
 * Find main content area
 */
function findMainContent(doc) {
  const selectors = [
    'article',
    'main',
    "[role='main']",
    '#content',
    '.content',
    '#main',
    '.main',
    '.post',
    '.entry-content',
    '.post-content',
    '.article-content',
    '.markdown-body', // GitHub
    '.documentation', // Docs sites
  ];

  for (const selector of selectors) {
    const el = doc.querySelector(selector);
    if (el && el.textContent?.trim().length > 100) {
      return el;
    }
  }

  return doc.body;
}

/**
 * Convert DOM element to markdown
 */
function convertElementToMarkdown(el, options, depth = 0) {
  if (!el) return '';

  const lines = [];
  processElement(el, options, lines, depth);
  return lines.join('\n');
}

/**
 * Process single element
 */
function processElement(el, options, lines, depth) {
  if (!el) return;

  // Text node
  if (el.nodeType === 3) {
    const text = el.textContent?.trim();
    if (text) lines.push(text);
    return;
  }

  // Skip non-elements
  if (el.nodeType !== 1) return;

  const tagName = el.tagName.toUpperCase();

  // Skip unwanted elements
  const skipTags = [
    'SCRIPT',
    'STYLE',
    'NOSCRIPT',
    'SVG',
    'IFRAME',
    'NAV',
    'HEADER',
    'FOOTER',
    'ASIDE',
  ];
  if (skipTags.includes(tagName)) return;

  // Skip by class/id patterns
  const className = (el.className?.toString() || '').toLowerCase();
  const id = (el.id || '').toLowerCase();
  const skipPatterns = [
    'nav',
    'menu',
    'sidebar',
    'ad',
    'advertisement',
    'banner',
    'social',
    'comment',
    'related',
  ];
  for (const pattern of skipPatterns) {
    if (
      (className.includes(pattern) || id.includes(pattern)) &&
      el.textContent?.trim().length < 500
    ) {
      return;
    }
  }

  // Handle specific tags
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
      const text = getInlineText(el, options);
      if (text.trim()) {
        lines.push('');
        lines.push(text);
        lines.push('');
      }
      return;
    }

    case 'A': {
      const text = el.textContent?.trim();
      const href = el.href;
      if (text && href && !href.startsWith('javascript:')) {
        lines.push(`[${text}](${href})`);
      } else if (text) {
        lines.push(text);
      }
      return;
    }

    case 'IMG': {
      if (options.includeImages) {
        const alt = el.alt || el.title || 'image';
        const src = el.src;
        if (src) lines.push(`![${alt}](${src})`);
      }
      return;
    }

    case 'UL':
    case 'OL': {
      lines.push('');
      processList(el, options, lines, tagName === 'OL', depth);
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
      if (el.parentElement?.tagName !== 'PRE') {
        const text = el.textContent?.trim();
        if (text) lines.push('`' + text + '`');
      }
      return;
    }

    case 'BLOCKQUOTE': {
      lines.push('');
      const quoteLines = [];
      for (const child of el.childNodes) {
        processElement(child, options, quoteLines, depth);
      }
      for (const line of quoteLines) {
        if (line.trim()) lines.push('> ' + line);
      }
      lines.push('');
      return;
    }

    case 'HR': {
      lines.push('');
      lines.push('---');
      lines.push('');
      return;
    }

    case 'STRONG':
    case 'B': {
      const text = el.textContent?.trim();
      if (text) lines.push('**' + text + '**');
      return;
    }

    case 'EM':
    case 'I': {
      const text = el.textContent?.trim();
      if (text) lines.push('*' + text + '*');
      return;
    }

    case 'TABLE': {
      const tableMarkdown = convertTable(el);
      if (tableMarkdown) {
        lines.push('');
        lines.push(tableMarkdown);
        lines.push('');
      }
      return;
    }

    default: {
      // Container elements - process children
      for (const child of el.childNodes) {
        processElement(child, options, lines, depth);
      }
    }
  }
}

/**
 * Get inline text content
 */
function getInlineText(el, _options) {
  let text = '';
  for (const child of el.childNodes) {
    if (child.nodeType === 3) {
      text += child.textContent;
    } else if (child.nodeType === 1) {
      const tag = child.tagName.toUpperCase();
      if (tag === 'A') {
        const href = child.href;
        const linkText = child.textContent?.trim();
        if (linkText && href && !href.startsWith('javascript:')) {
          text += `[${linkText}](${href})`;
        } else {
          text += linkText || '';
        }
      } else if (tag === 'STRONG' || tag === 'B') {
        text += '**' + child.textContent + '**';
      } else if (tag === 'EM' || tag === 'I') {
        text += '*' + child.textContent + '*';
      } else if (tag === 'CODE') {
        text += '`' + child.textContent + '`';
      } else if (tag === 'BR') {
        text += '  \n';
      } else {
        text += child.textContent;
      }
    }
  }
  return text.trim();
}

/**
 * Process list elements
 */
function processList(listEl, options, lines, isOrdered, depth) {
  const indent = '  '.repeat(depth);
  let counter = 1;

  for (const li of listEl.children) {
    if (li.tagName !== 'LI') continue;

    const prefix = isOrdered ? `${counter}. ` : '- ';
    const content = getInlineText(li, options);

    if (content.trim()) {
      lines.push(indent + prefix + content);
    }

    // Handle nested lists
    for (const child of li.children) {
      if (child.tagName === 'UL' || child.tagName === 'OL') {
        processList(child, options, lines, child.tagName === 'OL', depth + 1);
      }
    }

    counter++;
  }
}

/**
 * Convert table to markdown
 */
function convertTable(tableEl) {
  const rows = [];
  let headerRow = [];
  let hasHeader = false;

  for (const tr of tableEl.querySelectorAll('tr')) {
    const cells = tr.querySelectorAll('th, td');
    const rowData = [];

    for (const cell of cells) {
      rowData.push(cell.textContent?.trim().replace(/\|/g, '\\|') || '');
    }

    if (rowData.length === 0) continue;

    // First row with TH is header
    if (!hasHeader && tr.querySelector('th')) {
      headerRow = rowData;
      hasHeader = true;
      continue;
    }

    rows.push(rowData);
  }

  // Use first row as header if no header found
  if (headerRow.length === 0 && rows.length > 0) {
    headerRow = rows.shift();
  }

  if (headerRow.length === 0) return '';

  const colCount = headerRow.length;
  const lines = [];

  // Header
  lines.push('| ' + headerRow.join(' | ') + ' |');
  lines.push('| ' + headerRow.map(() => '---').join(' | ') + ' |');

  // Rows
  for (const row of rows) {
    while (row.length < colCount) row.push('');
    lines.push('| ' + row.slice(0, colCount).join(' | ') + ' |');
  }

  return lines.join('\n');
}

/**
 * Clean markdown output
 */
function cleanMarkdown(markdown) {
  return markdown
    .replace(/\n{3,}/g, '\n\n') // Remove excessive blank lines
    .replace(/[ \t]+$/gm, '') // Remove trailing whitespace
    .trim();
}

// =============================================================================
// Message Listener (Background API)
// =============================================================================

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
  const msgType = message.type;

  console.log('[NevoFlux] Background received:', msgType);

  // Handle Background API calls ("bg:" prefix)
  if (msgType && msgType.startsWith('bg:')) {
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

  // Handle AskUser response from sidebar
  if (msgType === MessageTypes.ASK_USER_RESPONSE) {
    handleAskUserResponse(message.payload);
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
      (async () => {
        try {
          const payload = message.payload;
          // Inject canvas context hint for chat messages
          if (payload?.type === 'chat_message' && payload?.payload?.content) {
            const hint = await getActiveCanvasHint();
            if (hint) {
              payload.payload.content = hint + '\n\n' + payload.payload.content;
            }
          }
          const sent = channelManager.sendToAgent(payload);
          sendResponse({ success: sent });
        } catch (e) {
          console.error('[NevoFlux] SEND_TO_AGENT error:', e);
          sendResponse({ success: false });
        }
      })();
      return true; // Keep sendResponse valid for async

    case BackgroundAPI.EXEC_TOOL:
      executeBrowserTool(message.payload, 'sidebar')
        .then((result) => sendResponse(result))
        .catch((err) =>
          sendResponse({
            success: false,
            error: { code: -1, message: err.message, recoverable: true },
          })
        );
      return true; // Keep sendResponse valid for async

    case BackgroundAPI.GET_TAB_CONTEXT: {
      // Support optional tab_id parameter to get specific tab context
      const requestedTabId = message.tab_id ?? null;
      console.log(
        '[NevoFlux] GET_TAB_CONTEXT requested, tab_id:',
        requestedTabId,
        'full message:',
        message
      );
      getTabContext(requestedTabId)
        .then((ctx) => {
          console.log('[NevoFlux] GET_TAB_CONTEXT returning:', ctx);
          sendResponse(ctx);
        })
        .catch(() => sendResponse(null));
      return true; // Keep sendResponse valid for async
    }

    case BackgroundAPI.SIDEBAR_CLOSE:
      browser.sidebarAction
        .close()
        .then(() => sendResponse({ success: true }))
        .catch((err) => {
          console.warn('[NevoFlux] Failed to close sidebar:', err);
          sendResponse({ success: false, error: err.message });
        });
      return true; // Keep sendResponse valid for async

    case BackgroundAPI.SIDEBAR_OPEN:
      browser.sidebarAction
        .open()
        .then(() => sendResponse({ success: true }))
        .catch((err) => {
          console.warn('[NevoFlux] Failed to open sidebar:', err);
          sendResponse({ success: false, error: err.message });
        });
      return true; // Keep sendResponse valid for async

    case BackgroundAPI.SIDEBAR_SET_WIDTH:
      // Fallback: try browser.nevoflux API (requires browser rebuild with new schema)
      // Primary path is direct DOM from sidebar WASM (nevoflux_api.rs)
      if (browser.nevoflux && browser.nevoflux.setSidebarWidth) {
        browser.nevoflux
          .setSidebarWidth(message.width || 500)
          .then(() => sendResponse({ success: true }))
          .catch((err) => {
            console.warn('[NevoFlux] Failed to set sidebar width:', err);
            sendResponse({ success: false, error: err.message });
          });
        return true;
      }
      console.warn('[NevoFlux] setSidebarWidth API not available (browser rebuild needed)');
      sendResponse({ success: false, error: 'setSidebarWidth API not available' });
      break;

    case 'bg:get_window_session': {
      const windowId = message.windowId;
      if (!windowId) {
        sendResponse({ success: false, error: 'windowId required' });
        return true;
      }
      getWindowSession(windowId)
        .then((sessionId) => {
          sendResponse({ success: true, sessionId });
        })
        .catch((e) => {
          sendResponse({ success: false, error: e.message });
        });
      return true;
    }

    case 'bg:set_window_session': {
      const { windowId: wId, sessionId: sId } = message;
      if (!wId || !sId) {
        sendResponse({ success: false, error: 'windowId and sessionId required' });
        return true;
      }
      setWindowSession(wId, sId)
        .then(() => {
          sendResponse({ success: true });
        })
        .catch((e) => {
          sendResponse({ success: false, error: e.message });
        });
      return true;
    }

    case 'bg:new_session': {
      const winId = message.windowId;
      if (!winId) {
        sendResponse({ success: false, error: 'windowId required' });
        return true;
      }
      const newSessionId = `sess-${Date.now().toString(16)}-${Math.random().toString(16).slice(2, 10)}`;
      setWindowSession(winId, newSessionId)
        .then(() => {
          sendResponse({ success: true, sessionId: newSessionId });
        })
        .catch((e) => {
          sendResponse({ success: false, error: e.message });
        });
      return true;
    }

    case 'bg:open_artifact': {
      const artifactId = message.id;
      if (!artifactId) {
        sendResponse({ success: false, error: 'id required' });
        return true;
      }

      // Hydrate ContentStore from backend if artifact not yet in memory
      (async () => {
        try {
          const existing = await browser.nevoflux.getArtifact(artifactId);
          if (!existing || !existing.success) {
            // Not in ContentStore — request from backend
            channelManager.sendToAgent({
              type: 'system_command',
              payload: {
                request_id: `art-get-${Date.now()}`,
                command: 'artifact.get',
                params: { artifact_id: artifactId },
              },
            });
          }
        } catch (e) {
          console.warn('[NevoFlux] getArtifact check failed, requesting from backend:', e);
          channelManager.sendToAgent({
            type: 'system_command',
            payload: {
              request_id: `art-get-${Date.now()}`,
              command: 'artifact.get',
              params: { artifact_id: artifactId },
            },
          });
        }
      })();

      // Reuse existing canvas tab if already open, otherwise open new one
      (async () => {
        try {
          if (_canvasTabId != null) {
            // Canvas tab already open — just activate it
            try {
              await browser.tabs.update(_canvasTabId, { active: true });
              // Navigate to the correct artifact if it changed
              await browser.tabs.update(_canvasTabId, {
                url: `nevoflux://canvas/${artifactId}`,
              });
              sendResponse({ success: true, tabId: _canvasTabId });
              return;
            } catch {
              // Tab was closed — fall through to open new one
              _canvasTabId = null;
            }
          }
          const result = await browser.nevoflux.openCanvasTab(artifactId);
          if (result?.success && result.tabId) {
            _canvasTabId = result.tabId;
          }
          sendResponse(result);
        } catch (e) {
          sendResponse({ success: false, error: e.message });
        }
      })();
      return true;
    }

    case BackgroundAPI.GET_SETTINGS: {
      (async () => {
        try {
          const key = message.key || 'settings';
          const result = await browser.nevoflux.getSettings(key);
          sendResponse({ success: true, data: result?.data || null });
        } catch (e) {
          console.error('[NevoFlux] GET_SETTINGS error:', e);
          sendResponse({ success: false, error: e.message });
        }
      })();
      return true;
    }

    default:
      console.warn('[NevoFlux] Unknown Background API:', apiType);
      sendResponse({ success: false, error: 'Unknown API' });
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
tabEventListeners.onActivated = async (_activeInfo) => {
  const context = await getActiveTabContext();
  broadcastToSidebar({
    type: MessageTypes.TAB_CONTEXT_UPDATE,
    payload: context,
  });
};
browser.tabs.onActivated.addListener(tabEventListeners.onActivated);

// Update tab context when tab URL changes
tabEventListeners.onUpdated = async (tabId, changeInfo, _tab) => {
  if (changeInfo.status === 'complete' || changeInfo.url) {
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

// Clear canvas tab tracking when the canvas tab is closed
browser.tabs.onRemoved.addListener((tabId) => {
  if (tabId === _canvasTabId) {
    _canvasTabId = null;
  }
});

// Cleanup function to remove event listeners (call on extension unload if needed)
function _cleanupTabEventListeners() {
  if (tabEventListeners.onActivated) {
    browser.tabs.onActivated.removeListener(tabEventListeners.onActivated);
    tabEventListeners.onActivated = null;
  }
  if (tabEventListeners.onUpdated) {
    browser.tabs.onUpdated.removeListener(tabEventListeners.onUpdated);
    tabEventListeners.onUpdated = null;
  }
  console.log('[NevoFlux] Tab event listeners cleaned up');
}

// =============================================================================
// New Tab Override — redirect about:newtab to nevoflux://home
// =============================================================================

browser.tabs.onCreated.addListener((tab) => {
  // New tabs start as about:newtab or about:blank before loading
  if (!tab.url || tab.url === 'about:newtab' || tab.url === 'about:home') {
    browser.tabs.update(tab.id, { url: 'nevoflux://home' }).catch((err) => {
      console.debug('[NevoFlux] Failed to redirect new tab:', err.message);
    });
  }
});

// =============================================================================
// Initialization
// =============================================================================

console.log('[NevoFlux] Background script initialized (Protocol v5.0 - 2-channel architecture)');
console.log('[NevoFlux] Channels: Chat (com.nevoflux.agent), MCP (com.nevoflux.agent.mcp)');
console.log('[NevoFlux] API namespace: bg:*');
