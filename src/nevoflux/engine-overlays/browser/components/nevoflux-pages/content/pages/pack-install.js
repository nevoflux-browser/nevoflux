/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/**
 * pack-install page controller. Parses ?src, previews via pack.inspect,
 * confirms, and installs with live progress. GitHub-only.
 *
 * Logic helpers are imported dynamically from pack-ui-logic.mjs (ES module);
 * the page itself is a classic script so it can run before module load.
 */
const PackInstall = {
  _logic: null,
  _parsed: null,

  async init() {
    this._logic = await import('chrome://nevoflux/content/pages/pack-ui-logic.mjs');
    this._wireStaticButtons();

    const src = new URLSearchParams(window.location.search).get('src');
    const parsed = this._logic.parsePackInstallSrc(src);
    if (!parsed.ok) {
      this._showError(parsed.error || 'Unrecognized pack source', src || '(none)', false);
      return;
    }
    this._parsed = parsed;
    // Task 5 fills in inspect/preview here. For now show the validated source.
    this._show('pi-loading');
    document.getElementById('pi-loading').textContent =
      `Validated source: ${parsed.display}`;
  },

  _wireStaticButtons() {
    const settings = () => { window.location.href = 'nevoflux://settings/packs'; };
    document.getElementById('pi-error-settings').addEventListener('click', settings);
    document.getElementById('pi-done-settings').addEventListener('click', settings);
    document.getElementById('pi-cancel').addEventListener('click', () => window.close());
    document.getElementById('pi-done-close').addEventListener('click', () => window.close());
  },

  _show(id) {
    for (const s of ['pi-loading', 'pi-error', 'pi-preview', 'pi-progress', 'pi-done']) {
      const el = document.getElementById(s);
      if (el) el.classList.toggle('pi-hidden', s !== id);
    }
    const loading = document.getElementById('pi-loading');
    if (loading) loading.style.display = id === 'pi-loading' ? '' : 'none';
  },

  _showError(msg, src, canRetry) {
    this._show('pi-error');
    document.getElementById('pi-error-msg').textContent = msg;
    document.getElementById('pi-error-src').textContent = src;
    document.getElementById('pi-error-retry').classList.toggle('pi-hidden', !canRetry);
  },
};

document.addEventListener('DOMContentLoaded', () => PackInstall.init());
