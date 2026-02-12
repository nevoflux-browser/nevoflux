/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

const { setTimeout, clearTimeout } = ChromeUtils.importESModule(
  "resource://gre/modules/Timer.sys.mjs"
);

/**
 * NevofluxBridgeRouter — shared parent-process singleton that connects
 * NevofluxParent (actor) with ext-nevoflux.js (extension API).
 *
 * Flow:
 *   NevofluxParent.request(type, payload)
 *     → handler(id, type, payload)        [set by ext-nevoflux.js]
 *       → background.js handles request
 *       → background.js calls browser.nevoflux.bridgeRespond(id, result)
 *         → ext-nevoflux.js calls NevofluxBridgeRouter.respond(id, result)
 *           → Promise resolves back to NevofluxParent
 */
export const NevofluxBridgeRouter = {
  _handler: null, // (id, type, payload) => void — set by ext-nevoflux.js
  _pending: new Map(), // id → { resolve, reject, timer }
  _counter: 0,

  REQUEST_TIMEOUT_MS: 30000,

  /**
   * Register the handler (called by ext-nevoflux.js when onBridgeRequest has a listener).
   */
  setHandler(fn) {
    this._handler = fn;
  },

  removeHandler() {
    this._handler = null;
  },

  /**
   * Make a request (called by NevofluxParent).
   * Returns a Promise that resolves when background.js calls bridgeRespond.
   */
  request(type, payload) {
    if (!this._handler) {
      return Promise.reject(
        new Error("NevofluxBridgeRouter: no handler registered")
      );
    }
    const id = `br_${++this._counter}_${Date.now()}`;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this._pending.delete(id);
        reject(new Error(`Bridge request timeout: ${type}`));
      }, this.REQUEST_TIMEOUT_MS);
      this._pending.set(id, { resolve, reject, timer });
      try {
        this._handler(id, type, payload);
      } catch (e) {
        clearTimeout(timer);
        this._pending.delete(id);
        reject(e);
      }
    });
  },

  /**
   * Resolve a pending request (called by ext-nevoflux.js bridgeRespond).
   */
  respond(id, result) {
    const entry = this._pending.get(id);
    if (entry) {
      clearTimeout(entry.timer);
      this._pending.delete(id);
      entry.resolve(result);
    }
  },

  // ── Push-based subscriptions (canvas agent:chat sessions) ──

  _subscriptions: new Map(), // sessionId → Set<callback>
  SESSION_TIMEOUT_MS: 300000, // 5 minutes
  _sessionTimers: new Map(),

  /**
   * Subscribe to push messages for a sessionId.
   * Returns an unsubscribe function.
   */
  subscribe(sessionId, callback) {
    if (!this._subscriptions.has(sessionId)) {
      this._subscriptions.set(sessionId, new Set());
    }
    this._subscriptions.get(sessionId).add(callback);
    this._touchSession(sessionId);
    return () => {
      const subs = this._subscriptions.get(sessionId);
      if (subs) {
        subs.delete(callback);
        if (subs.size === 0) {
          this._cleanupSession(sessionId);
        }
      }
    };
  },

  /**
   * Push a message to all subscribers of a sessionId.
   * Called by ext-nevoflux.js when background.js calls bridgePush.
   */
  push(sessionId, message) {
    const subs = this._subscriptions.get(sessionId);
    if (!subs || subs.size === 0) {
      return false;
    }
    this._touchSession(sessionId);
    for (const cb of subs) {
      try {
        cb(message);
      } catch (e) {
        subs.delete(cb);
      }
    }
    return true;
  },

  /**
   * Remove all subscriptions for a sessionId.
   */
  unsubscribe(sessionId) {
    this._cleanupSession(sessionId);
  },

  _touchSession(sessionId) {
    const existing = this._sessionTimers.get(sessionId);
    if (existing) clearTimeout(existing);
    const timer = setTimeout(() => {
      this._cleanupSession(sessionId);
    }, this.SESSION_TIMEOUT_MS);
    this._sessionTimers.set(sessionId, timer);
  },

  _cleanupSession(sessionId) {
    this._subscriptions.delete(sessionId);
    const timer = this._sessionTimers.get(sessionId);
    if (timer) {
      clearTimeout(timer);
      this._sessionTimers.delete(sessionId);
    }
  },
};
