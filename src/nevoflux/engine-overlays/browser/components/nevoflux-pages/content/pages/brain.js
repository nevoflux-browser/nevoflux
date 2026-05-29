/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/**
 * Brain — nevoflux://brain knowledge-base browse page (M4-4b).
 *
 * Backed by the M4-4a daemon brain.* RPCs:
 *   - brain.health  -> readiness probe ({ ok, brain_dir, reason? })
 *   - brain.list    -> page metadata ({ pages: [{slug,title,updated_at}] })
 *   - brain.get     -> full page ({ slug, title, compiled_truth, timeline, … })
 *
 * Envelope-unwrap mirrors Settings._sendMcpCommand (settings.js, M4-1)
 * verbatim — single helper, no duplication.
 */

// eslint-disable-next-line no-unused-vars -- referenced by DOMContentLoaded handler below
const Brain = {
  state: {
    health: null,
    pages: [],
    currentSlug: null,
    currentPage: null, // { slug, title, compiled_truth, timeline, ... } | { error }
    error: null,       // string | null  (backend/network error, NOT disabled state)
    loading: false,
    previewLoading: false,
  },

  // ── Lifecycle ───────────────────────────────────────────

  init() {
    this._wireToolbar();
    this._refresh();
  },

  _wireToolbar() {
    const refreshBtn = document.getElementById('brain-refresh-btn');
    if (refreshBtn) {
      refreshBtn.addEventListener('click', () => this._refresh());
    }
    // The settings link is an <a href="nevoflux://settings/knowledge-base"> —
    // the browser handles navigation natively. Nothing to bind.
  },

  // ── Data fetch ──────────────────────────────────────────

  async _refresh() {
    this.state.loading = true;
    this.state.error = null;
    this._renderListLoading();
    this._renderStatus();

    try {
      // brain.health always succeeds (envelope-wise) — disabled is a
      // first-class `ok=false` state, not an error.
      const health = await this._call('brain.health', {});
      this.state.health = health || {};

      if (health && health.ok) {
        try {
          const list = await this._call('brain.list', {});
          this.state.pages = (list && list.pages) || [];
        } catch (listErr) {
          this.state.pages = [];
          this.state.error = this._errMsg(listErr);
        }
      } else {
        // Disabled: clear pages, clear error (this is not an error state).
        this.state.pages = [];
      }
    } catch (e) {
      // brain.health itself failed — treat as backend error.
      this.state.health = null;
      this.state.pages = [];
      this.state.error = this._errMsg(e);
    } finally {
      this.state.loading = false;
      this._renderList();
      this._renderPreview();
      this._renderStatus();
    }
  },

  async _openPage(slug) {
    if (!slug) return;
    this.state.currentSlug = slug;
    this.state.currentPage = null;
    this.state.previewLoading = true;
    this._renderList(); // update active highlight
    this._renderPreview();

    try {
      const page = await this._call('brain.get', { slug });
      this.state.currentPage = page || {};
    } catch (e) {
      this.state.currentPage = { error: this._errMsg(e) };
    } finally {
      this.state.previewLoading = false;
      this._renderPreview();
    }
  },

  /**
   * Send a daemon RPC. Mirrors Settings._sendMcpCommand: double-unwrap
   * the {success, data} envelope and the inner agent system_response
   * payload, throwing a plain Error on either failure.
   *
   * Returns the inner `data` field (the actual RPC result) or {} if
   * the daemon returned a payload with no body.
   */
  async _call(command, params = {}) {
    const result = await NevofluxPage.sendQuery('bridge:request', {
      type: 'agent:command',
      payload: { command, params },
    });
    if (!result || !result.success) {
      throw new Error(
        (result && result.error && result.error.message) ||
          (result && typeof result.error === 'string' ? result.error : null) ||
          'Bridge request failed'
      );
    }
    const agentResponse = result.data;
    if (!agentResponse) {
      throw new Error('Empty agent response');
    }
    if (!agentResponse.success) {
      const err = agentResponse.error;
      throw new Error(
        (err && err.message) ||
          (typeof err === 'string' ? err : 'Agent command failed')
      );
    }
    return agentResponse.data || {};
  },

  _errMsg(e) {
    if (!e) return 'Unknown error';
    if (e instanceof Error) return e.message || String(e);
    return String(e);
  },

  // ── Render: page list ───────────────────────────────────

  _renderListLoading() {
    const list = document.getElementById('brain-list');
    if (!list) return;
    list.replaceChildren();
    const loading = document.createElement('div');
    loading.className = 'brain-list-loading';
    loading.textContent = 'Loading…';
    list.appendChild(loading);
  },

  _renderList() {
    const list = document.getElementById('brain-list');
    if (!list) return;
    list.replaceChildren();

    const health = this.state.health;
    if (!health || !health.ok) {
      // Disabled or no health — empty list, the preview pane shows the
      // user-facing explanation.
      const note = document.createElement('div');
      note.className = 'brain-list-empty';
      note.textContent = this.state.error
        ? 'Could not reach the daemon.'
        : 'Knowledge Base is not enabled.';
      list.appendChild(note);
      return;
    }

    if (this.state.pages.length === 0) {
      const note = document.createElement('div');
      note.className = 'brain-list-empty';
      note.textContent = 'No pages yet.';
      list.appendChild(note);
      return;
    }

    for (const p of this.state.pages) {
      list.appendChild(this._createListItem(p));
    }
  },

  _createListItem(page) {
    const item = document.createElement('div');
    item.className = 'brain-list-item';
    item.dataset.slug = page.slug || '';
    if (this.state.currentSlug && this.state.currentSlug === page.slug) {
      item.classList.add('brain-list-item--active');
    }

    const slug = document.createElement('div');
    slug.className = 'brain-list-item__slug';
    slug.textContent = page.slug || '(no slug)';
    item.appendChild(slug);

    if (page.title && page.title !== page.slug) {
      const title = document.createElement('div');
      title.className = 'brain-list-item__title';
      title.textContent = page.title;
      item.appendChild(title);
    }

    if (page.updated_at) {
      const time = document.createElement('div');
      time.className = 'brain-list-item__time';
      time.textContent = this._formatRelativeTime(page.updated_at);
      item.appendChild(time);
    }

    item.addEventListener('click', () => this._openPage(page.slug));
    return item;
  },

  /**
   * Best-effort relative time. Accepts ISO strings or epoch-seconds.
   * Falls back to the raw value if parsing fails.
   */
  _formatRelativeTime(value) {
    if (value == null) return '';
    let ts;
    if (typeof value === 'number') {
      // assume seconds if < 1e12, else ms
      ts = value < 1e12 ? value * 1000 : value;
    } else if (typeof value === 'string') {
      const parsed = Date.parse(value);
      if (Number.isNaN(parsed)) return value;
      ts = parsed;
    } else {
      return String(value);
    }
    const diff = Date.now() - ts;
    if (!Number.isFinite(diff)) return String(value);
    const sec = Math.round(diff / 1000);
    if (sec < 60) return 'just now';
    const min = Math.round(sec / 60);
    if (min < 60) return `${min}m ago`;
    const hr = Math.round(min / 60);
    if (hr < 24) return `${hr}h ago`;
    const day = Math.round(hr / 24);
    if (day < 30) return `${day}d ago`;
    return new Date(ts).toLocaleDateString();
  },

  // ── Render: preview pane ────────────────────────────────

  _renderPreview() {
    const preview = document.getElementById('brain-preview');
    if (!preview) return;
    preview.replaceChildren();

    // 1. Backend error (couldn't reach daemon — health probe failed)
    //    Disabled (`ok=false`) is NOT an error — it's handled below.
    if (this.state.error && !this._isDisabled()) {
      preview.appendChild(this._renderErrorPane(this.state.error));
      return;
    }

    // 2. Disabled
    if (this._isDisabled()) {
      preview.appendChild(this._renderDisabledPane());
      return;
    }

    // 3. Previewing a specific page
    if (this.state.currentSlug) {
      if (this.state.previewLoading) {
        const loading = document.createElement('div');
        loading.className = 'brain-preview-empty';
        loading.textContent = 'Loading page…';
        preview.appendChild(loading);
        return;
      }
      const page = this.state.currentPage;
      if (page && page.error) {
        preview.appendChild(this._renderErrorPane(page.error));
        return;
      }
      preview.appendChild(this._renderPagePane(page || {}));
      return;
    }

    // 4. Empty brain
    if (this.state.health && this.state.health.ok && this.state.pages.length === 0) {
      preview.appendChild(this._renderEmptyBrainPane());
      return;
    }

    // 5. Ready, no page selected
    preview.appendChild(this._renderNoSelectionPane());
  },

  _isDisabled() {
    const h = this.state.health;
    return !!h && h.ok === false;
  },

  _renderDisabledPane() {
    const wrap = document.createElement('div');
    wrap.className = 'brain-preview-empty';

    const title = document.createElement('h2');
    title.textContent = 'Knowledge Base is not yet enabled';
    wrap.appendChild(title);

    const desc = document.createElement('p');
    desc.textContent =
      'Open Settings to install gbrain and create your local brain.';
    wrap.appendChild(desc);

    const btn = document.createElement('a');
    btn.className = 'brain-btn brain-btn--primary';
    btn.href = 'nevoflux://settings/knowledge-base';
    btn.textContent = 'Open Settings to enable →';
    wrap.appendChild(btn);

    return wrap;
  },

  _renderErrorPane(msg) {
    const wrap = document.createElement('div');
    wrap.className = 'brain-preview-error';

    const text = document.createElement('p');
    text.textContent = `Could not reach the knowledge base (${msg}).`;
    wrap.appendChild(text);

    const retry = document.createElement('button');
    retry.type = 'button';
    retry.className = 'brain-btn brain-btn--primary';
    retry.textContent = 'Retry';
    retry.addEventListener('click', () => this._refresh());
    wrap.appendChild(retry);

    return wrap;
  },

  _renderEmptyBrainPane() {
    const wrap = document.createElement('div');
    wrap.className = 'brain-preview-empty';

    const title = document.createElement('h2');
    title.textContent = 'Your brain is empty';
    wrap.appendChild(title);

    const desc = document.createElement('p');
    desc.textContent =
      'Save your first page from the browser sidebar, or use ';
    const code1 = document.createElement('code');
    code1.textContent = 'gbrain put';
    desc.appendChild(code1);
    desc.appendChild(document.createTextNode(' from the command line.'));
    wrap.appendChild(desc);

    const hint = document.createElement('p');
    hint.className = 'brain-empty-hint';
    hint.appendChild(document.createTextNode('You can run '));
    const code2 = document.createElement('code');
    code2.textContent = 'gbrain stats';
    hint.appendChild(code2);
    hint.appendChild(
      document.createTextNode(' in a terminal to inspect the brain directly.')
    );
    wrap.appendChild(hint);

    return wrap;
  },

  _renderNoSelectionPane() {
    const wrap = document.createElement('div');
    wrap.className = 'brain-preview-empty';
    const title = document.createElement('h2');
    title.textContent = 'Select a page';
    wrap.appendChild(title);
    const desc = document.createElement('p');
    desc.textContent = 'Pick a page from the list to view its contents.';
    wrap.appendChild(desc);
    return wrap;
  },

  _renderPagePane(page) {
    const wrap = document.createElement('div');

    // Actions row
    const actions = document.createElement('div');
    actions.className = 'brain-preview-actions';
    const openBtn = document.createElement('button');
    openBtn.type = 'button';
    openBtn.className = 'brain-btn';
    openBtn.textContent = 'Open in external editor';
    openBtn.addEventListener('click', () => this._onOpenExternal(page));
    actions.appendChild(openBtn);
    wrap.appendChild(actions);

    // Title heading
    const title = document.createElement('h2');
    title.textContent = page.title || page.slug || '(untitled)';
    wrap.appendChild(title);

    // Main content
    const body = document.createElement('pre');
    body.textContent =
      page.compiled_truth != null
        ? String(page.compiled_truth)
        : '(no content)';
    wrap.appendChild(body);

    // Timeline (collapsed)
    if (page.timeline != null) {
      const details = document.createElement('details');
      const summary = document.createElement('summary');
      summary.textContent = 'Timeline';
      details.appendChild(summary);
      const timelinePre = document.createElement('pre');
      timelinePre.textContent =
        typeof page.timeline === 'string'
          ? page.timeline
          : JSON.stringify(page.timeline, null, 2);
      details.appendChild(timelinePre);
      wrap.appendChild(details);
    }

    return wrap;
  },

  _onOpenExternal(page) {
    // v1: just show the expected on-disk path. Real "open in editor"
    // intent lands once we expose a daemon RPC that resolves slug -> path.
    const slug = (page && page.slug) || '(unknown-slug)';
    const dir =
      (this.state.health && this.state.health.brain_dir) || '~/.gbrain';
    // eslint-disable-next-line no-alert -- v1 placeholder for editor-open
    alert(`Page file: ${dir}/pages/${slug}.md\n(External editor open is not yet wired.)`);
  },

  // ── Render: status bar ──────────────────────────────────

  _renderStatus() {
    const badge = document.getElementById('brain-status-badge');
    const count = document.getElementById('brain-status-count');
    const dir = document.getElementById('brain-status-dir');
    if (!badge || !count || !dir) return;

    // Reset badge classes
    badge.classList.remove(
      'brain-status__badge--ready',
      'brain-status__badge--disabled',
      'brain-status__badge--error'
    );

    if (this.state.loading) {
      badge.textContent = 'Loading…';
    } else if (this.state.error) {
      badge.textContent = 'Error';
      badge.classList.add('brain-status__badge--error');
    } else if (this._isDisabled()) {
      badge.textContent = 'Disabled';
      badge.classList.add('brain-status__badge--disabled');
    } else if (this.state.health && this.state.health.ok) {
      badge.textContent = 'Ready';
      badge.classList.add('brain-status__badge--ready');
    } else {
      badge.textContent = 'Unknown';
    }

    count.textContent = String(this.state.pages.length || 0);

    const brainDir =
      (this.state.health && this.state.health.brain_dir) || '—';
    dir.textContent = `brain_dir: ${brainDir}`;
  },
};

document.addEventListener('DOMContentLoaded', () => Brain.init());
