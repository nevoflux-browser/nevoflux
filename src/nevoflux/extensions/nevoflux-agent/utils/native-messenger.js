/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Native Messenger Utility
 * Wrapper for Native Messaging communication with NevoFlux backend
 */

const NATIVE_APP_ID = 'com.nevoflux.agent';

class NativeMessenger {
  constructor() {
    this.port = null;
    this.requestId = 0;
    this.pendingRequests = new Map();
    this.connectionListeners = [];
    this.messageListeners = [];
    this.isConnected = false;
  }

  /**
   * Connect to the native application
   */
  connect() {
    if (this.port) {
      console.warn('Native messenger already connected');
      return;
    }

    try {
      this.port = browser.runtime.connectNative(NATIVE_APP_ID);

      this.port.onMessage.addListener((message) => {
        this._handleMessage(message);
      });

      this.port.onDisconnect.addListener(() => {
        this._handleDisconnect();
      });

      this.isConnected = true;
      this._notifyConnectionListeners(true);
      console.log('Native messenger connected to:', NATIVE_APP_ID);
    } catch (error) {
      console.error('Failed to connect to native app:', error);
      this.isConnected = false;
      this._notifyConnectionListeners(false, error);
      throw error;
    }
  }

  /**
   * Disconnect from the native application
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
   * Send a message and wait for response
   * @param {Object} message - Message to send
   * @param {number} timeout - Timeout in milliseconds
   * @returns {Promise<Object>} Response from native app
   */
  async sendMessage(message, timeout = 30000) {
    if (!this.port || !this.isConnected) {
      this.connect();
    }

    const id = ++this.requestId;
    const request = {
      id,
      ...message,
    };

    return new Promise((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        this.pendingRequests.delete(id);
        reject(new Error('Native message timeout'));
      }, timeout);

      this.pendingRequests.set(id, {
        resolve: (response) => {
          clearTimeout(timeoutId);
          resolve(response);
        },
        reject: (error) => {
          clearTimeout(timeoutId);
          reject(error);
        },
      });

      try {
        this.port.postMessage(request);
      } catch (error) {
        clearTimeout(timeoutId);
        this.pendingRequests.delete(id);
        reject(error);
      }
    });
  }

  /**
   * Send a message without waiting for response
   * @param {Object} message - Message to send
   */
  postMessage(message) {
    if (!this.port || !this.isConnected) {
      this.connect();
    }

    const request = {
      id: ++this.requestId,
      ...message,
    };

    try {
      this.port.postMessage(request);
    } catch (error) {
      console.error('Failed to post message:', error);
      throw error;
    }
  }

  /**
   * Add a listener for incoming messages
   * @param {Function} callback - Message handler
   */
  onMessage(callback) {
    this.messageListeners.push(callback);
  }

  /**
   * Add a listener for connection state changes
   * @param {Function} callback - Connection state handler
   */
  onConnectionChange(callback) {
    this.connectionListeners.push(callback);
  }

  /**
   * Remove a message listener
   * @param {Function} callback - Listener to remove
   */
  removeMessageListener(callback) {
    const index = this.messageListeners.indexOf(callback);
    if (index > -1) {
      this.messageListeners.splice(index, 1);
    }
  }

  _handleMessage(message) {
    // Check if this is a response to a pending request
    if (message.id && this.pendingRequests.has(message.id)) {
      const { resolve, reject } = this.pendingRequests.get(message.id);
      this.pendingRequests.delete(message.id);

      if (message.error) {
        reject(new Error(message.error));
      } else {
        resolve(message);
      }
      return;
    }

    // Otherwise, notify all message listeners
    for (const listener of this.messageListeners) {
      try {
        listener(message);
      } catch (error) {
        console.error('Error in message listener:', error);
      }
    }
  }

  _handleDisconnect() {
    const error = browser.runtime.lastError;
    console.error('Native messenger disconnected:', error?.message || 'Unknown reason');

    this.port = null;
    this.isConnected = false;

    // Reject all pending requests
    for (const [_id, { reject }] of this.pendingRequests) {
      reject(new Error('Connection closed'));
    }
    this.pendingRequests.clear();

    this._notifyConnectionListeners(false, error);
  }

  _notifyConnectionListeners(connected, error = null) {
    for (const listener of this.connectionListeners) {
      try {
        listener(connected, error);
      } catch (err) {
        console.error('Error in connection listener:', err);
      }
    }
  }
}

// Export singleton instance
export const nativeMessenger = new NativeMessenger();
