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
  _inspect: null,
  _decision: null,

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
    await this._loadPreview();
  },

  async _sendMcpCommand(command, params = {}) {
    const result = await NevofluxPage.sendQuery('bridge:request', {
      type: 'agent:command',
      payload: { command, params },
    });
    if (!result || !result.success) {
      throw new Error(result?.error?.message || 'Bridge request failed');
    }
    const agentResponse = result.data;
    if (!agentResponse || !agentResponse.success) {
      throw new Error(agentResponse?.error?.message || 'Agent command failed');
    }
    return agentResponse.data;
  },

  async _loadPreview() {
    this._show('pi-loading');
    document.getElementById('pi-loading').textContent = 'Inspecting pack…';
    let inspect, listData;
    try {
      [inspect, listData] = await Promise.all([
        this._sendMcpCommand('pack.inspect', this._logic.inspectParams(this._parsed.source)),
        this._sendMcpCommand('pack.list', {}),
      ]);
    } catch (e) {
      this._showError(this._logic.packErrorMessage(e), this._parsed.display, true);
      return;
    }
    this._inspect = inspect;

    const summary = this._logic.summarizeInspect(inspect);
    const name = (inspect && inspect.pack && inspect.pack.name) || this._parsed.display;
    const incomingVersion = (inspect && inspect.pack && String(inspect.pack.version)) || '';
    const rows = this._logic.packListToRows(listData);
    const installed = this._logic.findInstalledVersion(rows, name);
    this._decision = this._logic.decidePackAction(installed, incomingVersion);

    this._renderPreview(name, summary);
  },

  _renderPreview(name, summary) {
    this._show('pi-preview');
    document.getElementById('pi-name').textContent = name;
    document.getElementById('pi-src').textContent = this._parsed.display;
    document.getElementById('pi-summary').textContent = summary.text;

    const vio = document.getElementById('pi-violations');
    const primary = document.getElementById('pi-primary');
    if (summary.hasViolations) {
      vio.classList.remove('pi-hidden');
      vio.textContent = `Blocked: capability violations (${summary.violations.length}). This pack tries to escape the sandbox and cannot be installed.`;
      primary.disabled = true;
      primary.textContent = 'Install';
      return;
    }
    vio.classList.add('pi-hidden');
    primary.disabled = false;

    const labels = {
      install: 'Install',
      update: `Update (${this._decision.currentVersion} → ${(this._inspect.pack && this._inspect.pack.version) || ''})`,
      reinstall: 'Reinstall (already latest)',
      downgrade: `Reinstall (downgrade to ${(this._inspect.pack && this._inspect.pack.version) || ''})`,
    };
    primary.textContent = labels[this._decision.action] || 'Install';
    // Task 6 wires primary.onclick.
  },

  _wireStaticButtons() {
    const settings = () => { window.location.href = 'nevoflux://settings/packs'; };
    document.getElementById('pi-error-settings').addEventListener('click', settings);
    document.getElementById('pi-done-settings').addEventListener('click', settings);
    document.getElementById('pi-cancel').addEventListener('click', () => window.close());
    document.getElementById('pi-done-close').addEventListener('click', () => window.close());
    document.getElementById('pi-error-retry').addEventListener('click', () => this._loadPreview());
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
