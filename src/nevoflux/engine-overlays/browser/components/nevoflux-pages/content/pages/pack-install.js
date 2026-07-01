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
  _rawSrc: '',

  async init() {
    this._logic = await import('chrome://nevoflux/content/pages/pack-ui-logic.mjs');
    this._wireStaticButtons();

    const src = new URLSearchParams(window.location.search).get('src');
    this._rawSrc = src || ''; // reported to the website on a successful install
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
    primary.onclick = () => this._runInstall();
  },

  _installCall() {
    const src = this._parsed.source;
    const a = this._decision.action;
    if (a === 'update') {
      return this._sendMcpCommand('pack.update', { ...this._logic.updateParams(src), wait: false });
    }
    // install (fresh) | reinstall | downgrade → install, force for the latter two
    const force = a === 'reinstall' || a === 'downgrade';
    return this._sendMcpCommand('pack.install', this._logic.installParams(src, { wait: false, force }));
  },

  async _runInstall() {
    document.getElementById('pi-primary').disabled = true;
    this._show('pi-progress');

    const channelId = 'packinst_' + Date.now() + '_' + Math.random().toString(36).slice(2, 8);
    let opId = '';
    let subscriptionId = null;
    let messageListener = null;

    const bar = document.getElementById('pi-bar');
    const phase = document.getElementById('pi-phase');

    try {
      await NevofluxPage.sendQuery('events:channel_open', { channelId });

      const completion = new Promise((resolve, reject) => {
        messageListener = (event) => {
          const detail = event.detail;
          if (!detail || detail.type !== 'bridge:push') return;
          const msg = detail.msg;
          if (!msg || msg.type !== 'events:delivery') return;
          const ev = msg.payload && msg.payload.event;
          if (!ev || ev.topic !== 'system:pack:progress') return;
          // op_id is ours — latch it to avoid dropping frames that arrive
          // before the install reply returns the op_id.
          if (!opId && ev.payload && ev.payload.op_id) opId = ev.payload.op_id;
          const view = this._logic.summarizePackProgress(ev.payload, opId);
          if (!view.matched) return;
          bar.value = view.pct;
          phase.textContent = view.line;
          if (view.terminal) {
            view.ok ? resolve() : reject(new Error(view.line || 'install failed'));
          }
        };
        window.addEventListener('NevofluxMessage', messageListener);
      });

      const sub = await NevofluxPage.sendQuery('bridge:request', {
        type: 'events.subscribe',
        payload: { patterns: ['system:pack:progress'], replay_sticky: false, channel_id: channelId },
      });
      if (!sub || sub.success === false) {
        throw new Error(sub?.error?.message || 'events.subscribe failed');
      }
      const subData = sub.data?.data !== undefined ? sub.data.data : sub.data;
      subscriptionId = subData?.subscription_id || subData?.subscriptionId || null;

      const started = await this._installCall();
      opId = (started && (started.op_id || started.opId)) || opId;
      if (!opId) throw new Error('daemon did not return an op_id for wait:false install');

      await completion;
      this._reportInstall();
      this._showDone();
    } catch (e) {
      this._showError(this._logic.packErrorMessage(e), this._parsed.display, true);
    } finally {
      if (messageListener) window.removeEventListener('NevofluxMessage', messageListener);
      try {
        if (subscriptionId) {
          await NevofluxPage.sendQuery('bridge:request', {
            type: 'events.unsubscribe', payload: { subscription_id: subscriptionId },
          });
        }
        await NevofluxPage.sendQuery('events:channel_close', { channelId });
      } catch (_) {}
    }
  },

  /**
   * Best-effort: tell the website an install succeeded so it can bump the
   * pack's install count. Keyed by the raw `src` (equals pack.install_src).
   * Cross-origin from chrome://, fire-and-forget — failures are non-fatal and
   * must never block the install UI.
   */
  _reportInstall() {
    const src = this._rawSrc;
    if (!src) return;
    try {
      fetch('https://nevoflux.app/api/packs/install', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ src }),
      }).catch(() => {});
    } catch (_) {}
  },

  _showDone() {
    this._show('pi-done');
    const name = (this._inspect && this._inspect.pack && this._inspect.pack.name) || this._parsed.display;
    const comps = (this._inspect && this._inspect.components) || {};
    const nSkills = Array.isArray(comps.skills) ? comps.skills.length : 0;
    const nTools = Array.isArray(comps.canvas_tools) ? comps.canvas_tools.length : 0;
    const nSeed = Array.isArray(comps.seed) ? comps.seed.length : 0;
    const parts = [`${nSkills} skill(s)`, `${nTools} canvas tool(s)`];
    if (nSeed) parts.push(`${nSeed} seed page(s)`);
    if (comps.dashboard) parts.push('a dashboard');
    if (comps.knowledge) parts.push('a knowledge base');
    document.getElementById('pi-done-title').textContent = `${name} installed`;
    document.getElementById('pi-done-msg').textContent = `Installed ${parts.join(', ')}.`;
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
