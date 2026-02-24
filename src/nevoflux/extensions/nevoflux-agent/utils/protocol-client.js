/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Protocol Client for Browser Agent Native Messaging V2.0
 * Implements the complete protocol specification with envelope structure
 */

const NATIVE_APP_ID = 'com.nevoflux.agent';
const PROTOCOL_VERSION = '2.0';

/**
 * Generate a UUID v4
 * @returns {string} UUID v4 string
 */
function generateUuid() {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function (c) {
    const r = (Math.random() * 16) | 0;
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

/**
 * Protocol Client for managing communication with Rust Agent
 */
export class ProtocolClient {
  constructor() {
    this.port = null;
    this.sessionId = generateUuid();
    this.isConnected = false;

    // Message handlers
    this.streamHandlers = new Map();
    this.uiHandlers = new Map();
    this.connectionListeners = [];
    this.browserControlHandlers = [];

    // Current tab info
    this.currentTabId = null;
    this.currentUrl = null;
    this.currentTitle = null;
  }

  /**
   * Connect to native messaging host
   */
  connect() {
    if (this.port) {
      console.warn('[Protocol] Already connected');
      return;
    }

    try {
      this.port = browser.runtime.connectNative(NATIVE_APP_ID);

      this.port.onMessage.addListener((envelope) => {
        this._handleIncomingMessage(envelope);
      });

      this.port.onDisconnect.addListener(() => {
        this._handleDisconnect();
      });

      this.isConnected = true;
      this._notifyConnectionListeners(true);
      console.log('[Protocol] Connected to:', NATIVE_APP_ID);
    } catch (error) {
      console.error('[Protocol] Failed to connect:', error);
      this.isConnected = false;
      this._notifyConnectionListeners(false, error);
      throw error;
    }
  }

  /**
   * Disconnect from native host
   */
  disconnect() {
    if (this.port) {
      this.port.disconnect();
      this.port = null;
      this.isConnected = false;
      this._notifyConnectionListeners(false);
    }
  }

  /**
   * Create a new envelope
   * @param {string} msgType - Message type (e.g., "input.chat")
   * @param {Object} payload - Message payload
   * @returns {Object} Envelope structure
   */
  _createEnvelope(msgType, payload) {
    return {
      ver: PROTOCOL_VERSION,
      msg_id: generateUuid(),
      session_id: this.sessionId,
      type: msgType,
      payload: payload,
      timestamp: Math.floor(Date.now() / 1000),
    };
  }

  /**
   * Send envelope to native host
   * @param {Object} envelope - Envelope to send
   */
  _sendEnvelope(envelope) {
    if (!this.port || !this.isConnected) {
      this.connect();
    }

    try {
      this.port.postMessage(envelope);
      console.log('[Protocol] Sent:', envelope.type);
    } catch (error) {
      console.error('[Protocol] Send error:', error);
      throw error;
    }
  }

  // ==========================================================================
  // Uplink Messages: Extension -> Rust Agent
  // ==========================================================================

  /**
   * Send chat input
   * @param {string} text - Message text
   * @param {Array} files - Optional file attachments [{name, mime, data}]
   * @param {Object} contextRef - Optional context reference
   */
  sendChat(text, files = [], contextRef = null) {
    const payload = {
      text,
      files,
    };

    if (contextRef) {
      payload.context_ref = contextRef;
    }

    const envelope = this._createEnvelope('input.chat', payload);
    this._sendEnvelope(envelope);

    return envelope.msg_id;
  }

  /**
   * Send UI event (button click, form submission)
   * @param {string} viewId - Source view ID
   * @param {string} actionId - Action ID triggered
   * @param {Object} formData - Optional form data
   */
  sendUiEvent(viewId, actionId, formData = {}) {
    const payload = {
      view_id: viewId,
      action_id: actionId,
      form_data: formData,
    };

    const envelope = this._createEnvelope('input.ui_event', payload);
    this._sendEnvelope(envelope);
  }

  /**
   * Send command
   * @param {string} cmd - Command name
   * @param {Object} args - Command arguments
   */
  sendCommand(cmd, args = {}) {
    const payload = {
      cmd,
      args,
    };

    const envelope = this._createEnvelope('input.command', payload);
    this._sendEnvelope(envelope);
  }

  /**
   * Send tab update
   * @param {number} tabId - Tab ID
   * @param {string} url - Current URL
   * @param {string} title - Page title
   * @param {string} status - Loading status
   */
  sendTabUpdate(tabId, url, title, status) {
    this.currentTabId = tabId;
    this.currentUrl = url;
    this.currentTitle = title;

    const payload = {
      tab_id: tabId,
      url,
      title,
      status,
    };

    const envelope = this._createEnvelope('context.tab_update', payload);
    this._sendEnvelope(envelope);
  }

  // ==========================================================================
  // Downlink Message Handlers
  // ==========================================================================

  /**
   * Handle incoming message from native host
   * @param {Object} envelope - Received envelope
   */
  _handleIncomingMessage(envelope) {
    console.log('[Protocol] Received:', envelope.type);

    try {
      switch (envelope.type) {
        case 'agent.stream.text':
          this._handleStreamText(envelope.payload);
          break;

        case 'agent.ui.render':
          this._handleUiRender(envelope.payload);
          break;

        case 'agent.ui.update':
          this._handleUiUpdate(envelope.payload);
          break;

        case 'browser.control':
          this._handleBrowserControl(envelope.payload);
          break;

        default:
          console.warn('[Protocol] Unknown message type:', envelope.type);
      }
    } catch (error) {
      console.error('[Protocol] Error handling message:', error);
    }
  }

  /**
   * Handle text stream message
   * @param {Object} payload - Stream payload
   */
  _handleStreamText(payload) {
    const { stream_id, delta, finish } = payload;

    // Notify stream handlers
    for (const [id, handler] of this.streamHandlers) {
      try {
        handler({
          streamId: stream_id,
          delta,
          finish,
          format: payload.format || 'markdown',
        });
      } catch (error) {
        console.error('[Protocol] Stream handler error:', error);
      }
    }
  }

  /**
   * Handle UI render message
   * @param {Object} payload - Render payload
   */
  _handleUiRender(payload) {
    const { route, view_id, layout } = payload;

    // Notify UI handlers
    for (const [id, handler] of this.uiHandlers) {
      try {
        handler({
          action: 'render',
          route,
          viewId: view_id,
          layout,
        });
      } catch (error) {
        console.error('[Protocol] UI handler error:', error);
      }
    }
  }

  /**
   * Handle UI update message
   * @param {Object} payload - Update payload
   */
  _handleUiUpdate(payload) {
    const { view_id, target_component_id, props } = payload;

    // Notify UI handlers
    for (const [id, handler] of this.uiHandlers) {
      try {
        handler({
          action: 'update',
          viewId: view_id,
          componentId: target_component_id,
          props,
        });
      } catch (error) {
        console.error('[Protocol] UI handler error:', error);
      }
    }
  }

  /**
   * Handle browser control message
   * @param {Object} payload - Control payload
   */
  _handleBrowserControl(payload) {
    for (const handler of this.browserControlHandlers) {
      try {
        handler(payload);
      } catch (error) {
        console.error('[Protocol] Browser control handler error:', error);
      }
    }
  }

  // ==========================================================================
  // Event Listeners
  // ==========================================================================

  /**
   * Add stream message handler
   * @param {string} id - Handler ID
   * @param {Function} handler - Handler function
   */
  onStream(id, handler) {
    this.streamHandlers.set(id, handler);
  }

  /**
   * Remove stream handler
   * @param {string} id - Handler ID
   */
  offStream(id) {
    this.streamHandlers.delete(id);
  }

  /**
   * Add UI message handler
   * @param {string} id - Handler ID
   * @param {Function} handler - Handler function
   */
  onUi(id, handler) {
    this.uiHandlers.set(id, handler);
  }

  /**
   * Remove UI handler
   * @param {string} id - Handler ID
   */
  offUi(id) {
    this.uiHandlers.delete(id);
  }

  /**
   * Add browser control handler
   * @param {Function} handler - Handler function
   */
  onBrowserControl(handler) {
    this.browserControlHandlers.push(handler);
  }

  /**
   * Add connection state change listener
   * @param {Function} callback - Callback function
   */
  onConnectionChange(callback) {
    this.connectionListeners.push(callback);
  }

  /**
   * Handle disconnect event
   */
  _handleDisconnect() {
    const error = browser.runtime.lastError;
    console.error('[Protocol] Disconnected:', error?.message || 'Unknown');

    this.port = null;
    this.isConnected = false;

    this._notifyConnectionListeners(false, error);
  }

  /**
   * Notify connection listeners
   * @param {boolean} connected - Connection state
   * @param {Error} error - Optional error
   */
  _notifyConnectionListeners(connected, error = null) {
    for (const listener of this.connectionListeners) {
      try {
        listener(connected, error);
      } catch (err) {
        console.error('[Protocol] Connection listener error:', err);
      }
    }
  }

  /**
   * Get current session ID
   * @returns {string} Session ID
   */
  getSessionId() {
    return this.sessionId;
  }

  /**
   * Reset session ID (creates new session)
   */
  resetSession() {
    this.sessionId = generateUuid();
    console.log('[Protocol] New session:', this.sessionId);
  }
}

// Export singleton instance
export const protocolClient = new ProtocolClient();
