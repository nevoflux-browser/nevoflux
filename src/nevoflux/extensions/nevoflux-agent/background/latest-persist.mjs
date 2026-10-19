/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Write-through persistence that always ends on the latest value of a key.
 *
 * A streamed Canvas is written many times in quick succession (each delta,
 * then the completed artifact). The old scheme skipped any write while one
 * was in flight for the key, so the final value could be dropped and the
 * daemon kept a snapshot from the middle of the stream: a Canvas that
 * reopened truncated. Here a write that arrives while one is in flight
 * becomes the key's pending value (a newer one replaces it), and is written
 * as soon as the current write settles.
 */
export class LatestPersister {
  /** `write(operation, key, value)` resolves once the daemon acknowledged it. */
  constructor(write) {
    this.write = write;
    this.chains = new Map(); // key -> { promise, pending, current }
  }

  /** Whether a write for `key` is in flight or queued. */
  busy(key) {
    return this.chains.has(key);
  }

  /**
   * Persist `value` (or a newer value of `key`). Resolves once the latest
   * value queued for the key is written; rejects when that last write fails.
   * A value equal to the one being written is not queued again.
   */
  persist(key, operation, value) {
    const entry = { operation, value, text: fingerprint(operation, value) };
    const chain = this.chains.get(key);
    if (chain) {
      if (chain.current.text !== entry.text) {
        chain.pending = entry;
      } else {
        chain.pending = null;
      }
      return chain.promise;
    }
    const state = { pending: null, current: entry, promise: null };
    state.promise = this.drain(key, state);
    this.chains.set(key, state);
    // Callers that ignore the promise must not see an unhandled rejection.
    state.promise.catch(() => {});
    return state.promise;
  }

  async drain(key, state) {
    try {
      for (;;) {
        let failure = null;
        try {
          await this.write(state.current.operation, key, state.current.value);
        } catch (err) {
          failure = err;
        }
        if (!state.pending) {
          if (failure) {
            throw failure;
          }
          return;
        }
        state.current = state.pending;
        state.pending = null;
      }
    } finally {
      if (this.chains.get(key) === state) {
        this.chains.delete(key);
      }
    }
  }
}

function fingerprint(operation, value) {
  if (operation !== 'set') {
    return operation;
  }
  try {
    return `set:${JSON.stringify(value)}`;
  } catch {
    // Unserializable: never equal to anything, so it is always written.
    return Symbol('unserializable');
  }
}
