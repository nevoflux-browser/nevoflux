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
    // Pagination + filter (M-pagination). Drives the paginated brain.list
    // { q, sort, offset, limit } -> { pages, total, offset, limit } contract.
    offset: 0,
    limit: 50,
    q: '',
    sort: 'updated_desc',
    total: 0,
  },

  // ── Lifecycle ───────────────────────────────────────────

  init() {
    this._wireToolbar();
    this._refresh();

    // Wire the share / import / shares dialogs (M5-C). Kept in their own
    // objects below; this just binds the buttons + auto-open behavior.
    BrainShareUI.init();
  },

  _wireToolbar() {
    const refreshBtn = document.getElementById('brain-refresh-btn');
    if (refreshBtn) {
      refreshBtn.addEventListener('click', () => this._refresh());
    }

    const shareBtn = document.getElementById('brain-share-btn');
    if (shareBtn) {
      shareBtn.addEventListener('click', () => BrainShareDialog.open());
    }
    const importBtn = document.getElementById('brain-import-btn');
    if (importBtn) {
      importBtn.addEventListener('click', () => BrainImportDialog.open());
    }
    const sharesBtn = document.getElementById('brain-shares-btn');
    if (sharesBtn) {
      sharesBtn.addEventListener('click', () => BrainSharesList.open());
    }
    // The settings link is an <a href="nevoflux://settings/knowledge-base"> —
    // the browser handles navigation natively. Nothing to bind.

    // Filter bar + pagination controls (M-pagination).
    const qInput = document.getElementById('brain-q');
    if (qInput) {
      qInput.addEventListener('input', () => {
        this.state.q = qInput.value;
        this.state.offset = 0; // new filter resets to page 1
        this._debouncedRefresh();
      });
    }
    const sortSel = document.getElementById('brain-sort');
    if (sortSel) {
      sortSel.addEventListener('change', () => {
        this.state.sort = sortSel.value;
        this.state.offset = 0;
        this._refresh();
      });
    }
    const prevBtn = document.getElementById('brain-prev');
    if (prevBtn) {
      prevBtn.addEventListener('click', () => {
        this.state.offset = Math.max(0, this.state.offset - this.state.limit);
        this._refresh();
      });
    }
    const nextBtn = document.getElementById('brain-next');
    if (nextBtn) {
      nextBtn.addEventListener('click', () => {
        if (this.state.offset + this.state.limit < this.state.total) {
          this.state.offset += this.state.limit;
          this._refresh();
        }
      });
    }
  },

  // Debounce the search input so each keystroke doesn't fire a brain.list.
  _debouncedRefresh() {
    clearTimeout(this._qTimer);
    this._qTimer = setTimeout(() => this._refresh(), 200);
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
          const list = await this._call('brain.list', {
            q: this.state.q,
            sort: this.state.sort,
            offset: this.state.offset,
            limit: this.state.limit,
          });
          this.state.pages = (list && list.pages) || [];
          this.state.total =
            list && typeof list.total === 'number'
              ? list.total
              : this.state.pages.length;
        } catch (listErr) {
          this.state.pages = [];
          this.state.total = 0;
          this.state.error = this._errMsg(listErr);
        }
      } else {
        // Disabled: clear pages, clear error (this is not an error state).
        this.state.pages = [];
        this.state.total = 0;
      }
    } catch (e) {
      // brain.health itself failed — treat as backend error.
      this.state.health = null;
      this.state.pages = [];
      this.state.total = 0;
      this.state.error = this._errMsg(e);
    } finally {
      this.state.loading = false;
      this._renderList();
      this._renderPagination();
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

  // ── Render: pagination controls ─────────────────────────

  _renderPagination() {
    const bar = document.getElementById('brain-pagination');
    const info = document.getElementById('brain-pageinfo');
    const prev = document.getElementById('brain-prev');
    const next = document.getElementById('brain-next');
    if (!bar || !info || !prev || !next) return;

    const ready = this.state.health && this.state.health.ok;
    const total = this.state.total || 0;
    const limit = this.state.limit || 50;
    // Hide pagination entirely when disabled or everything fits one page.
    if (!ready || total <= limit) {
      bar.hidden = true;
      return;
    }
    bar.hidden = false;
    const pageCount = Math.max(1, Math.ceil(total / limit));
    const currentPage = Math.floor(this.state.offset / limit) + 1;
    info.textContent = `Page ${currentPage} of ${pageCount}`;
    prev.disabled = this.state.offset <= 0;
    next.disabled = this.state.offset + limit >= total;
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

    count.textContent = String(this.state.total || 0);

    const brainDir =
      (this.state.health && this.state.health.brain_dir) || '—';
    dir.textContent = `brain_dir: ${brainDir}`;
  },
};

// ════════════════════════════════════════════════════════════════════
//  Brain Share UI (M5-C) — mirrors canvas.js ShareDialog/ImportDialog.
//
//  All daemon calls reuse Brain._call(method, params), which dispatches
//  through the SAME agent:command path as brain.health/brain.list/brain.get.
//  Confirmed in nevoflux-agent server.rs: "brain.share_create" etc. sit in
//  the same command switch as "brain.health".
//
//  RPC shapes (confirmed against brain_share_rpc.rs / brain_share/service.rs):
//   - brain.share_create     { files?[], directory?, compiled_only?, frontmatter_whitelist?[], title?, ttl_secs? }
//                            -> { share_id, share_url, expires_at, size_bytes }
//   - brain.share_import_url { url, source_name?, trust? ("read_only"|"full_merge") }
//                            -> { files_imported, conflicts }
//   - brain.share_list       {}  -> { shares: [{share_id, share_url, title, expires_at, size_bytes, created_at}] }
//   - brain.share_renew      { share_id, extend_secs? } -> { expires_at }
//   - brain.share_revoke     { share_id } -> { revoked }
// ════════════════════════════════════════════════════════════════════

// Generic step toggler shared by both dialogs (mirrors ShareDialog._showStep).
function _brainShowStep(dialog, step) {
  // All known step ids across both dialogs; we hide every one then show `step`.
  const steps = [
    'confirm', 'loading', 'result', 'error',
    'prompt', 'preview', 'success', 'onboarding',
  ];
  steps.forEach((s) => {
    const el = document.getElementById(`${dialog}-step-${s}`);
    if (el) el.hidden = s !== step;
  });

  // Drive the stepper indicator (01/02/03). 'error'/'onboarding' don't advance.
  const flows = {
    brainshare: ['confirm', 'loading', 'result'],
    brainimport: ['prompt', 'preview', 'success'],
  };
  const flow = flows[dialog];
  const stepper = document.getElementById(`${dialog}-stepper`);
  if (!flow || !stepper) return;
  // 'loading' on import is not a stepper node; map it to the preview index so
  // the indicator stays sensible mid-flight.
  let idxStep = step;
  if (dialog === 'brainimport' && step === 'loading') idxStep = 'preview';
  const activeIdx = flow.indexOf(idxStep);
  if (activeIdx < 0) return;
  stepper.querySelectorAll('li').forEach((li, i) => {
    li.classList.toggle('is-active', i === activeIdx);
    li.classList.toggle('is-done', i < activeIdx);
  });
}

function _brainShowDialog(which) {
  const el = document.getElementById(`nevoflux-${which}-dialog`);
  if (el) el.hidden = false;
}
function _brainCloseDialog(which) {
  const el = document.getElementById(`nevoflux-${which}-dialog`);
  if (el) el.hidden = true;
}

// ── Share-create dialog ─────────────────────────────────────────────
const BrainShareDialog = {
  open() {
    _brainShowDialog('brainshare');
    _brainShowStep('brainshare', 'confirm');
    this._resetForm();
    this._renderPages();
  },

  _resetForm() {
    const compiled = document.getElementById('brainshare-compiled-only');
    if (compiled) compiled.checked = true;
    const fm = document.getElementById('brainshare-fm-whitelist');
    if (fm) fm.value = '';
    const title = document.getElementById('brainshare-title-input');
    if (title) title.value = '';
  },

  _renderPages() {
    const box = document.getElementById('brainshare-pages');
    if (!box) return;
    box.replaceChildren();

    const pages = (Brain.state && Brain.state.pages) || [];
    if (!pages.length) {
      const empty = document.createElement('div');
      empty.className = 'brain-pageselect-empty';
      empty.textContent =
        Brain._isDisabled && Brain._isDisabled()
          ? 'Knowledge Base is not enabled.'
          : 'No pages — the whole brain will be shared.';
      box.appendChild(empty);
      this._updateCount();
      return;
    }

    for (const p of pages) {
      const label = document.createElement('label');
      label.className = 'brain-pageselect-item';

      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.className = 'brainshare-page-cb';
      cb.value = p.slug || '';
      cb.addEventListener('change', () => this._updateCount());
      label.appendChild(cb);

      const slug = document.createElement('span');
      slug.className = 'ps-slug';
      slug.textContent = p.slug || '(no slug)';
      label.appendChild(slug);

      if (p.title && p.title !== p.slug) {
        const t = document.createElement('span');
        t.className = 'ps-title';
        t.textContent = `· ${p.title}`;
        label.appendChild(t);
      }
      box.appendChild(label);
    }
    this._updateCount();
  },

  _selectedSlugs() {
    return Array.from(document.querySelectorAll('.brainshare-page-cb'))
      .filter((cb) => cb.checked)
      .map((cb) => cb.value)
      .filter(Boolean);
  },

  _updateCount() {
    const count = document.getElementById('brainshare-count');
    if (!count) return;
    const n = this._selectedSlugs().length;
    count.textContent = n === 0 ? 'all pages' : `${n} selected`;
  },

  selectAll(on) {
    document
      .querySelectorAll('.brainshare-page-cb')
      .forEach((cb) => { cb.checked = on; });
    this._updateCount();
  },

  async confirm() {
    _brainShowStep('brainshare', 'loading');
    try {
      const files = this._selectedSlugs();
      const compiledOnly = !!document.getElementById('brainshare-compiled-only').checked;
      const fmRaw = (document.getElementById('brainshare-fm-whitelist').value || '').trim();
      const fmWhitelist = fmRaw
        ? fmRaw.split(',').map((s) => s.trim()).filter(Boolean)
        : [];
      const title = (document.getElementById('brainshare-title-input').value || '').trim();

      const params = { compiled_only: compiledOnly };
      // `files: []` means "whole brain" on the daemon side (Selection::Directory("")).
      if (files.length) params.files = files;
      if (fmWhitelist.length) params.frontmatter_whitelist = fmWhitelist;
      if (title) params.title = title;

      const result = await Brain._call('brain.share_create', params);

      document.getElementById('brainshare-url-input').value = result.share_url || '';
      const expiresEl = document.getElementById('brainshare-expires-date');
      expiresEl.textContent = result.expires_at
        ? new Date(result.expires_at * 1000).toLocaleString()
        : '—';
      const sizeEl = document.getElementById('brainshare-size');
      sizeEl.textContent = _brainFormatBytes(result.size_bytes);

      _brainShowStep('brainshare', 'result');
    } catch (err) {
      document.getElementById('brainshare-error-msg').textContent =
        Brain._errMsg(err);
      _brainShowStep('brainshare', 'error');
    }
  },

  copyLink() {
    const input = document.getElementById('brainshare-url-input');
    if (input) navigator.clipboard.writeText(input.value).catch(() => {});
  },
};

// ── Manage-shares dialog ────────────────────────────────────────────
const BrainSharesList = {
  open() {
    _brainShowDialog('brainshares');
    this.load();
  },

  async load() {
    const list = document.getElementById('brainshares-list');
    if (!list) return;
    list.replaceChildren();
    const loading = document.createElement('div');
    loading.className = 'brain-shares-empty';
    loading.textContent = 'Loading…';
    list.appendChild(loading);

    let shares = [];
    try {
      const res = await Brain._call('brain.share_list', {});
      shares = (res && res.shares) || [];
    } catch (err) {
      list.replaceChildren();
      const e = document.createElement('div');
      e.className = 'brain-shares-empty';
      e.textContent = `Could not load shares (${Brain._errMsg(err)}).`;
      list.appendChild(e);
      return;
    }
    this._render(shares);
  },

  _render(shares) {
    const list = document.getElementById('brainshares-list');
    if (!list) return;
    list.replaceChildren();

    if (!shares.length) {
      const empty = document.createElement('div');
      empty.className = 'brain-shares-empty';
      empty.textContent = 'No shares yet. Use “Share…” to create one.';
      list.appendChild(empty);
      return;
    }

    for (const s of shares) {
      list.appendChild(this._row(s));
    }
  },

  _row(s) {
    const row = document.createElement('div');
    row.className = 'brain-share-row';
    row.dataset.shareId = s.share_id || '';

    const title = document.createElement('div');
    title.className = 'brain-share-row__title';
    title.textContent = s.title || s.share_id || '(untitled share)';
    row.appendChild(title);

    const url = document.createElement('div');
    url.className = 'brain-share-row__url';
    url.textContent = s.share_url || '';
    url.title = s.share_url || '';
    row.appendChild(url);

    const meta = document.createElement('div');
    meta.className = 'brain-share-row__meta';
    const expires = document.createElement('span');
    expires.textContent = `Expires ${
      s.expires_at ? new Date(s.expires_at * 1000).toLocaleDateString() : '—'
    }`;
    meta.appendChild(expires);
    const size = document.createElement('span');
    size.textContent = _brainFormatBytes(s.size_bytes);
    meta.appendChild(size);
    row.appendChild(meta);

    const actions = document.createElement('div');
    actions.className = 'brain-share-row__actions';

    const copyBtn = document.createElement('button');
    copyBtn.type = 'button';
    copyBtn.className = 'nevoflux-btn-secondary';
    copyBtn.textContent = 'Copy';
    copyBtn.addEventListener('click', () => {
      if (s.share_url) navigator.clipboard.writeText(s.share_url).catch(() => {});
    });
    actions.appendChild(copyBtn);

    const renewBtn = document.createElement('button');
    renewBtn.type = 'button';
    renewBtn.className = 'nevoflux-btn-secondary';
    renewBtn.textContent = 'Renew';
    renewBtn.addEventListener('click', () => this._renew(s.share_id, renewBtn));
    actions.appendChild(renewBtn);

    const revokeBtn = document.createElement('button');
    revokeBtn.type = 'button';
    revokeBtn.className = 'nevoflux-btn-secondary';
    revokeBtn.textContent = 'Revoke';
    revokeBtn.addEventListener('click', () => this._revoke(s.share_id, revokeBtn));
    actions.appendChild(revokeBtn);

    row.appendChild(actions);
    return row;
  },

  async _renew(shareId, btn) {
    if (!shareId) return;
    const orig = btn.textContent;
    btn.disabled = true;
    btn.textContent = 'Renewing…';
    try {
      // extend_secs omitted -> daemon defaults to 30 days.
      await Brain._call('brain.share_renew', { share_id: shareId });
      await this.load();
    } catch (err) {
      btn.disabled = false;
      btn.textContent = orig;
      // eslint-disable-next-line no-alert -- lightweight inline feedback
      alert(`Renew failed: ${Brain._errMsg(err)}`);
    }
  },

  async _revoke(shareId, btn) {
    if (!shareId) return;
    // eslint-disable-next-line no-alert -- confirm destructive action
    if (!confirm('Revoke this share? The link will stop working immediately.')) {
      return;
    }
    const orig = btn.textContent;
    btn.disabled = true;
    btn.textContent = 'Revoking…';
    try {
      await Brain._call('brain.share_revoke', { share_id: shareId });
      await this.load();
    } catch (err) {
      btn.disabled = false;
      btn.textContent = orig;
      // eslint-disable-next-line no-alert -- lightweight inline feedback
      alert(`Revoke failed: ${Brain._errMsg(err)}`);
    }
  },
};

// ── Import dialog ───────────────────────────────────────────────────
const BrainImportDialog = {
  _url: null,

  open(prefillUrl) {
    _brainShowDialog('brainimport');
    const input = document.getElementById('brainimport-url-input');
    if (input) input.value = prefillUrl || '';
    const src = document.getElementById('brainimport-source-input');
    if (src) src.value = 'shared';
    const trust = document.getElementById('brainimport-trust-select');
    if (trust) trust.value = 'read_only';
    _brainShowStep('brainimport', 'prompt');
    if (prefillUrl) {
      // Auto-advance to the review step when launched from a share link.
      this.next();
    }
  },

  // Step 1 -> 2: validate the URL has an id + #key fragment, then preview.
  // NOTE: the daemon exposes no manifest-preview / dry-run RPC, so we cannot
  // show the bundle's real title/description/file-list before importing.
  // We surface the parsed share_id here and present files_imported / conflicts
  // + the strip-rules audit on the success step. A true pre-import manifest
  // preview would need a new `brain.share_preview` RPC (not invented here).
  next() {
    const input = document.getElementById('brainimport-url-input');
    const url = (input && input.value || '').trim();
    if (!url) return;
    const parsed = this._parseUrl(url);
    if (!parsed) {
      document.getElementById('brainimport-error-msg').textContent =
        'That does not look like a brain share link (missing “#key” fragment).';
      _brainShowStep('brainimport', 'error');
      return;
    }
    this._url = url;
    document.getElementById('brainimport-share-id').textContent = parsed.shareId;
    _brainShowStep('brainimport', 'preview');
  },

  back() {
    _brainShowStep('brainimport', 'prompt');
  },

  // Split a share URL into { shareId } — mirrors service.rs parse_share_url:
  // accepts `.../b/<id>#<key>`; requires a non-empty id and a #fragment.
  _parseUrl(url) {
    const hash = url.indexOf('#');
    if (hash < 0) return null;
    const base = url.slice(0, hash);
    const fragment = url.slice(hash + 1);
    if (!fragment) return null;
    const segs = base.split('/').filter(Boolean);
    const shareId = segs.length ? segs[segs.length - 1] : '';
    if (!shareId) return null;
    return { shareId };
  },

  async submit() {
    if (!this._url) return;

    // Receiver onboarding: if the brain is disabled, don't error — guide the
    // user to enable the Knowledge Base first. Re-probe health to be current.
    try {
      const health = await Brain._call('brain.health', {});
      if (!health || health.ok === false) {
        _brainShowStep('brainimport', 'onboarding');
        return;
      }
    } catch (_e) {
      // Health probe itself failed — fall through and let import surface the
      // real backend error rather than masking it as onboarding.
    }

    const sourceName =
      (document.getElementById('brainimport-source-input').value || '').trim() ||
      'shared';
    const trust =
      document.getElementById('brainimport-trust-select').value === 'full_merge'
        ? 'full_merge'
        : 'read_only';

    _brainShowStep('brainimport', 'loading');
    try {
      const result = await Brain._call('brain.share_import_url', {
        url: this._url,
        source_name: sourceName,
        trust,
      });

      const filesImported =
        result && result.files_imported != null ? result.files_imported : 0;
      const conflicts = (result && result.conflicts) || [];
      document.getElementById('brainimport-files-count').textContent =
        String(filesImported);
      const conflictCount = Array.isArray(conflicts)
        ? conflicts.length
        : Number(conflicts) || 0;
      document.getElementById('brainimport-conflicts-count').textContent =
        String(conflictCount);

      // Strip-rules / conflict audit area.
      const audit = document.getElementById('brainimport-success-audit');
      if (audit) {
        if (Array.isArray(conflicts) && conflicts.length) {
          audit.hidden = false;
          audit.textContent =
            'Conflicts:\n' +
            conflicts
              .map((c) => (typeof c === 'string' ? c : JSON.stringify(c)))
              .join('\n');
        } else {
          audit.hidden = true;
        }
      }

      _brainShowStep('brainimport', 'success');
    } catch (err) {
      document.getElementById('brainimport-error-msg').textContent =
        Brain._errMsg(err);
      _brainShowStep('brainimport', 'error');
    }
  },

  retry() {
    _brainShowStep('brainimport', 'prompt');
  },

  done() {
    _brainCloseDialog('brainimport');
    // Refresh the page list so freshly-imported pages appear.
    Brain._refresh();
  },
};

function _brainFormatBytes(bytes) {
  const n = Number(bytes);
  if (!Number.isFinite(n) || n <= 0) return '—';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

// Wiring for all three dialogs. Mirrors canvas.js's DOMContentLoaded block.
const BrainShareUI = {
  init() {
    // Close buttons / backdrops for every modal.
    document.querySelectorAll('[data-close-dialog]').forEach((el) => {
      el.addEventListener('click', () => {
        const dialog = el.closest('.nevoflux-modal');
        if (dialog) dialog.hidden = true;
      });
    });

    // Share-create dialog.
    this._bind('brainshare-confirm-btn', () => BrainShareDialog.confirm());
    this._bind('brainshare-retry-btn', () => _brainShowStep('brainshare', 'confirm'));
    this._bind('brainshare-copy-url-btn', () => BrainShareDialog.copyLink());
    this._bind('brainshare-copy-link-btn', () => BrainShareDialog.copyLink());
    this._bind('brainshare-select-all', () => BrainShareDialog.selectAll(true));
    this._bind('brainshare-select-none', () => BrainShareDialog.selectAll(false));

    // Manage-shares dialog.
    this._bind('brainshares-refresh-btn', () => BrainSharesList.load());

    // Import dialog.
    this._bind('brainimport-next-btn', () => BrainImportDialog.next());
    this._bind('brainimport-back-btn', () => BrainImportDialog.back());
    this._bind('brainimport-submit-btn', () => BrainImportDialog.submit());
    this._bind('brainimport-retry-btn', () => BrainImportDialog.retry());
    this._bind('brainimport-done-btn', () => BrainImportDialog.done());

    const importUrl = document.getElementById('brainimport-url-input');
    if (importUrl) {
      importUrl.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          BrainImportDialog.next();
        }
      });
    }

    // Drag-drop of a local .nbrain file onto the import URL step.
    // TODO(M5-C file-import): the daemon exposes only brain.share_import_url
    // (URL path). There is no brain.share_import_file / import-from-bytes RPC,
    // so dropping a local .nbrain cannot be wired without inventing an RPC.
    // We detect the drop and explain this instead of silently no-op'ing.
    const promptStep = document.getElementById('brainimport-step-prompt');
    if (promptStep) {
      promptStep.addEventListener('dragover', (e) => e.preventDefault());
      promptStep.addEventListener('drop', (e) => {
        e.preventDefault();
        const f = e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files[0];
        if (f && /\.nbrain$/i.test(f.name)) {
          document.getElementById('brainimport-error-msg').textContent =
            'Local .nbrain file import is not available yet — paste a share ' +
            'URL instead. (File import needs a daemon RPC that does not exist.)';
          _brainShowStep('brainimport', 'error');
        }
      });
    }

    // Auto-open import flow when launched with a share URL/param:
    //   nevoflux://brain?import=<url>   or   nevoflux://brain?url=<url>
    this._maybeAutoImport();
  },

  _bind(id, fn) {
    const el = document.getElementById(id);
    if (el) el.addEventListener('click', fn);
  },

  _maybeAutoImport() {
    let url = null;
    try {
      if (typeof NevofluxPage !== 'undefined' && NevofluxPage.getParam) {
        url = NevofluxPage.getParam('import') || NevofluxPage.getParam('url') || null;
      }
      if (!url) {
        const u = new URL(window.location.href);
        url = u.searchParams.get('import') || u.searchParams.get('url') || null;
      }
    } catch (_e) {
      url = null;
    }
    if (url) {
      BrainImportDialog.open(url);
    }
  },
};

document.addEventListener('DOMContentLoaded', () => Brain.init());
