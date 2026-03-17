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
// eslint-disable-next-line no-unused-vars -- used by settings.js, home.js, canvas.js
const NevofluxPage = {
  /**
   * Parse query parameters from current URL.
   * Handles both chrome://...?section=llm and nevoflux://settings/llm formats.
   * The protocol handler sets originalURI so window.location shows the nevoflux:// URL,
   * which has path segments instead of query params.
   * @returns {URLSearchParams}
   */
  getParams() {
    const loc = window.location;
    // If we have query params, use them directly (chrome:// resolved URL)
    if (loc.search) {
      return new URLSearchParams(loc.search);
    }
    // For nevoflux:// URLs, derive params from the path segments
    // using the same routing logic as NevofluxProtocolHandler
    if (loc.protocol === 'nevoflux:') {
      const host = loc.hostname;
      const segments = loc.pathname.replace(/^\/+/, '').split('/').filter(Boolean);
      const params = new URLSearchParams();
      switch (host) {
        case 'canvas':
          if (segments[0]) params.set('id', segments[0]);
          params.set('mode', segments[1] || 'preview');
          break;
        case 'settings':
          params.set('section', segments[0] || 'general');
          break;
        case 'plan':
          if (segments[0]) params.set('id', segments[0]);
          break;
      }
      return params;
    }
    return new URLSearchParams();
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
