/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/**
 * NevofluxPage -- shared utilities for all nevoflux:// pages.
 *
 * Provides:
 * - Actor message passing (sendQuery/sendAsyncMessage to NevofluxChild)
 * - URL query parameter parsing
 * - Event helpers
 */
const NevofluxPage = {
  /**
   * Parse query parameters from current URL.
   * @returns {URLSearchParams}
   */
  getParams() {
    return new URLSearchParams(window.location.search);
  },

  /**
   * Get a single query parameter.
   * @param {string} name
   * @param {string} [defaultValue]
   * @returns {string|null}
   */
  getParam(name, defaultValue = null) {
    return this.getParams().get(name) ?? defaultValue;
  },

  /**
   * Send a query to the NevofluxChild actor and await a response.
   * Only available in chrome:// privileged context.
   *
   * @param {string} messageName
   * @param {object} data
   * @returns {Promise<any>}
   */
  async sendQuery(messageName, data = {}) {
    const actor = this._getActor();
    if (!actor) {
      throw new Error('NevofluxChild actor not available');
    }
    return actor.sendQuery(messageName, data);
  },

  /**
   * Send an async (fire-and-forget) message to the NevofluxChild actor.
   *
   * @param {string} messageName
   * @param {object} data
   */
  sendMessage(messageName, data = {}) {
    const actor = this._getActor();
    if (!actor) {
      console.warn('NevofluxChild actor not available for message:', messageName);
      return;
    }
    actor.sendAsyncMessage(messageName, data);
  },

  /**
   * Get the NevofluxChild actor for this window.
   * @returns {JSWindowActorChild|null}
   * @private
   */
  _getActor() {
    try {
      return window.windowGlobalChild?.getActor('Nevoflux');
    } catch (e) {
      console.error('Failed to get Nevoflux actor:', e);
      return null;
    }
  },
};
