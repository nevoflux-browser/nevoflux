/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * NevofluxContentStore -- Parent-process in-memory cache for artifact data.
 *
 * Provides a key-value store with subscription support. Keys follow the
 * convention: "canvas:{id}", "session:{id}", "config:{key}".
 *
 * Persistence: Write-through to Rust Agent SQLite via extension bridge.
 *   ContentStore.set() -> _notifyPersist() -> ext-nevoflux.js onContentStoreChanged
 *   -> background.js -> SystemCommand -> Rust Agent -> SQLite
 */
export const NevofluxContentStore = {
  /** @type {Map<string, any>} */
  _data: new Map(),

  /** @type {Map<string, Set<function>>} */
  _subscribers: new Map(),

  /** @type {Set<function(string, string, any): void>} */
  _persistCallbacks: new Set(),

  /** @type {boolean} True while bulk-loading from persistence (suppresses _notifyPersist) */
  _loading: false,

  /**
   * Get a value by key.
   * @param {string} key
   * @returns {any|undefined}
   */
  get(key) {
    const entry = this._data.get(key);
    return entry ? structuredClone(entry) : undefined;
  },

  /**
   * Set a value by key. Notifies subscribers and persistence callbacks.
   * @param {string} key
   * @param {any} value
   */
  set(key, value) {
    const cloned = structuredClone(value);
    this._data.set(key, cloned);
    this._notify(key, cloned);
    this._notifyPersist('set', key, cloned);
  },

  /**
   * Delete a key. Notifies subscribers and persistence callbacks.
   * @param {string} key
   * @returns {boolean}
   */
  delete(key) {
    const existed = this._data.delete(key);
    if (existed) {
      this._notify(key, undefined);
      this._notifyPersist('delete', key);
    }
    return existed;
  },

  /**
   * Query all entries matching a key prefix.
   * @param {string} prefix
   * @returns {Array<{key: string, value: any}>}
   */
  query(prefix) {
    const results = [];
    for (const [key, value] of this._data) {
      if (key.startsWith(prefix)) {
        results.push({ key, value: structuredClone(value) });
      }
    }
    return results;
  },

  /**
   * Subscribe to changes on a key.
   * @param {string} key
   * @param {function(any): void} callback
   * @returns {function(): void} unsubscribe function
   */
  subscribe(key, callback) {
    if (!this._subscribers.has(key)) {
      this._subscribers.set(key, new Set());
    }
    this._subscribers.get(key).add(callback);

    return () => {
      const subs = this._subscribers.get(key);
      if (subs) {
        subs.delete(callback);
        if (subs.size === 0) {
          this._subscribers.delete(key);
        }
      }
    };
  },

  /**
   * Register a persistence callback. Called on every set/delete unless _loading.
   * @param {function(string, string, any): void} callback - (operation, key, value)
   * @returns {function(): void} unsubscribe function
   */
  onPersist(callback) {
    this._persistCallbacks.add(callback);
    return () => {
      this._persistCallbacks.delete(callback);
    };
  },

  /**
   * Notify subscribers for a key.
   * @param {string} key
   * @param {any} value
   * @private
   */
  _notify(key, value) {
    const subs = this._subscribers.get(key);
    if (subs) {
      for (const cb of subs) {
        try {
          cb(structuredClone(value));
        } catch (e) {
          console.error(`ContentStore subscriber error for key "${key}":`, e);
        }
      }
    }
  },

  /**
   * Notify persistence callbacks. Skipped when _loading is true.
   * @param {string} op - "set" or "delete"
   * @param {string} key
   * @param {any} [value]
   * @private
   */
  _notifyPersist(op, key, value) {
    if (this._loading) {
      return;
    }
    for (const cb of this._persistCallbacks) {
      try {
        cb(op, key, value);
      } catch (e) {
        console.error(`ContentStore persist callback error for key "${key}":`, e);
      }
    }
  },

  /**
   * Clear all data and subscribers. For testing only.
   */
  _reset() {
    this._data.clear();
    this._subscribers.clear();
    this._persistCallbacks.clear();
    this._loading = false;
  },
};
