/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/**
 * Settings page controller.
 *
 * Renders section-based settings UI with form controls. Settings are
 * persisted via ContentStore (config:{key} namespace) through the actor.
 */

const NEW_TOOL_TEMPLATE = `# Canvas Tool definition — see docs/reference/skills/app/SKILL.md
name = "my-tool"
description = "One-line summary shown to the LLM and in the settings list."
kind = "command"          # "command" | "internal"
binary = "/usr/bin/echo"  # required when kind = "command"
args_mode = "template"    # "template" | "free"
args = ["{{message}}"]

[params.message]
type = "text"
optional = false

[constraints]
timeout_seconds = 30
`;

const Settings = {
  _currentSection: 'general',
  _saveTimer: null,
  _settings: {},
  _mcpServers: [],
  _mdSections: [
    {
      key: 'identity',
      filename: 'IDENTITY.md',
      title: 'Identity',
      desc: 'Define who you are \u2014 your name, creature type, vibe, and avatar.',
      hasAvatar: true,
      defaultContent:
        "## Basic Info\n\n**Name:** NevoFlux\n**Creature:** AI companion\n**Vibe:** Helpful and curious\n**Emoji:** \uD83E\uDD16\n\n## About Me\n\nI'm here to assist you with your tasks and make your workflow smoother.",
      placeholder: '',
    },
    {
      key: 'soul',
      filename: 'SOUL.md',
      title: 'Soul',
      desc: 'Your core truths, values, and behavioral principles.',
      defaultContent:
        '## Core Truths\n\n### 1. Help genuinely, not performatively\n- No "Great question!" or "I\'d be happy to help!"\n- Just do the thing\n\n### 2. Have opinions\n- It\'s okay to disagree, have preferences\n\n### 3. Try before asking\n- Read files, check context, search first\n\n## Boundaries\n\n- Private stuff stays private\n- When uncertain, ask before acting externally',
      placeholder: '',
    },
    {
      key: 'user',
      filename: 'USER.md',
      title: 'User',
      desc: "Information about the human you're helping.",
      defaultContent:
        '## Basic Info\n\n**Name:**\n**Pronouns:**\n**Timezone:**\n\n## Notes\n\n## Context\n\nWhat do they care about?\nWhat are they working on?',
      placeholder: '',
    },
    {
      key: 'tools',
      filename: 'TOOLS.md',
      title: 'Tools',
      desc: 'Environment-specific configurations: cameras, SSH hosts, TTS preferences, device nicknames.',
      defaultContent: '',
      placeholder:
        '## Environment Configuration\n\n### SSH Hosts\n- home-server \u2192 192.168.1.100, user: admin\n\n### Devices\n- iphone \u2192 Personal phone\n\n### TTS Preferences\n- Preferred voice: "Nova"',
    },
    {
      key: 'agents',
      filename: 'AGENTS.md',
      title: 'Agents Runtime',
      desc: 'Runtime instructions, memory management rules, safety protocols.',
      defaultContent: '',
      placeholder:
        "## Every Session - Read These First\n\n1. SOUL.md \u2014 this is who you are\n2. USER.md \u2014 this is who you're helping\n\n## Memory System\n\n## Safety\n- Never leak private data\n- Ask before destructive commands",
    },
  ],
  _mdOriginal: {},
  _mdDirty: false,

  init() {
    this._currentSection = NevofluxPage.getParam('section', 'general');
    this._setupNavigation();
    this._renderSections();
    this._activateSection(this._currentSection);
    this._loadSettings();
    this._loadSouls();
  },

  // ── Navigation ──────────────────────────────────────────

  _setupNavigation() {
    const nav = document.getElementById('settings-nav');
    nav.addEventListener('click', (e) => {
      const link = e.target.closest('a[data-section]');
      if (!link) return;
      e.preventDefault();
      const section = link.dataset.section;
      this._activateSection(section);
    });
  },

  _activateSection(sectionId) {
    this._currentSection = sectionId;

    // Update nav
    for (const link of document.querySelectorAll('.settings-nav a')) {
      link.classList.toggle('active', link.dataset.section === sectionId);
    }

    // Show/hide sections
    for (const section of document.querySelectorAll('.settings-section')) {
      section.classList.toggle('active', section.id === `section-${sectionId}`);
    }
  },

  // ── Section Rendering ───────────────────────────────────

  _renderSections() {
    const container = document.getElementById('settings-content');
    container.innerHTML = '';

    container.appendChild(this._renderGeneralSection());
    container.appendChild(this._renderLLMSection());
    container.appendChild(this._renderSoulsSection());
    container.appendChild(this._renderMcpSection());
    container.appendChild(this._renderCanvasToolsSection());
    container.appendChild(this._renderMyCanvasSection());
    container.appendChild(this._renderKnowledgeBaseSection());
    container.appendChild(this._renderPacksSection());
    container.appendChild(this._renderShortcutsSection());
  },

  // ── General Section ─────────────────────────────────────

  _renderGeneralSection() {
    const section = this._createSection('general', 'General');

    // ── Sidebar & Agent (structured, auto-save) ──
    const sidebarGroup = this._createGroup('Sidebar & Agent');
    sidebarGroup.appendChild(
      this._createSelectRow(
        'Sidebar default',
        'general.sidebarBehavior',
        [
          ['default', 'Default (remember last state)'],
          ['auto', 'Auto-open'],
          ['manual', 'Manual only'],
        ],
        'default'
      )
    );
    sidebarGroup.appendChild(
      this._createSelectRow(
        'Agent execution',
        'general.agentExecution',
        // Canonical tiers + migration: agent-execution-tiers.mjs (node-tested).
        // Ascending privilege; each tier auto-approves cumulatively more.
        [
          ['read-only', 'Read-only'],
          ['browser-auto', 'Browser auto'],
          ['browser-auto-local-read', 'Browser auto + reads'],
          ['full-auto', 'Full auto'],
        ],
        'read-only'
      )
    );
    sidebarGroup.appendChild(
      this._createTextRow(
        'Data storage',
        'general.dataPath',
        '~/.config/nevoflux/',
        'Path for artifact and session data'
      )
    );
    section.appendChild(sidebarGroup);

    section.appendChild(this._renderSpeechModelsGroup());
    section.appendChild(this._renderSpeechVoiceGroup());

    // ── Markdown config sections ──
    for (const md of this._mdSections) {
      const group = this._createGroup(md.title);

      const desc = document.createElement('p');
      desc.className = 'section-desc';
      desc.textContent = md.desc;
      group.appendChild(desc);

      // Avatar row for Identity
      if (md.hasAvatar) {
        group.appendChild(this._createAvatarRow('Avatar', 'identity.avatar'));
      }

      const textarea = document.createElement('textarea');
      textarea.className = 'markdown-textarea';
      textarea.id = `md-${md.key}`;
      textarea.placeholder = md.placeholder;
      textarea.addEventListener('input', () => this._checkMdDirty());
      group.appendChild(textarea);

      section.appendChild(group);
    }

    // ── Save bar ──
    const saveBar = document.createElement('div');
    saveBar.className = 'save-bar';
    saveBar.id = 'md-save-bar';

    const saveBtn = document.createElement('button');
    saveBtn.className = 'save-btn';
    saveBtn.type = 'button';
    saveBtn.id = 'md-save-btn';
    saveBtn.textContent = 'Save All Settings';
    saveBtn.disabled = true;
    saveBtn.addEventListener('click', () => this._saveMdFiles());
    saveBar.appendChild(saveBtn);

    const saveStatus = document.createElement('span');
    saveStatus.className = 'save-status';
    saveStatus.id = 'md-save-status';
    saveBar.appendChild(saveStatus);

    section.appendChild(saveBar);

    return section;
  },

  // ── LLM Section ─────────────────────────────────────────

  _llmProviders: [],
  _customLogic: null,

  /** Lazily load the DOM-free custom-provider logic (see the .mjs module). */
  async _ensureCustomLogic() {
    if (!this._customLogic) {
      this._customLogic = await import(
        'chrome://nevoflux/content/pages/custom-provider-logic.mjs'
      );
    }
    return this._customLogic;
  },

  /** Lazily load the DOM-free on-device logic (see the .mjs module). */
  async _ensureLocalLogic() {
    if (!this._localLogic) {
      this._localLogic = await import(
        'chrome://nevoflux/content/pages/local-inference-logic.mjs'
      );
    }
    return this._localLogic;
  },

  _renderLLMSection() {
    const section = this._createSection('llm', 'AI Models');

    // Guidance banner (hidden by default)
    const banner = document.createElement('div');
    banner.className = 'llm-guidance-banner';
    banner.id = 'llm-guidance-banner';
    banner.style.display = 'none';
    section.appendChild(banner);

    // On-device group, above the provider grid: it is the only provider whose
    // setup is a download rather than an API key, so it needs its own surface.
    section.appendChild(this._renderLocalGroup());

    // LLM Providers group (service + local)
    const providerGroup = this._createGroup('LLM Providers');
    const grid = document.createElement('div');
    grid.className = 'llm-providers-grid';
    grid.id = 'llm-providers-grid';
    const loading = document.createElement('div');
    loading.className = 'llm-loading';
    loading.id = 'llm-loading';
    loading.textContent = 'Loading providers...';
    grid.appendChild(loading);
    providerGroup.appendChild(grid);
    section.appendChild(providerGroup);

    // Agents group (cli + agent)
    const agentGroup = this._createGroup('Agents');
    const agentGrid = document.createElement('div');
    agentGrid.className = 'llm-providers-grid';
    agentGrid.id = 'llm-agents-grid';
    agentGroup.appendChild(agentGrid);
    section.appendChild(agentGroup);

    // Custom Providers group (user-defined endpoints)
    const customGroup = this._createGroup('Custom Providers');
    const customDesc = document.createElement('p');
    customDesc.className = 'section-desc';
    customDesc.textContent =
      'Point NevoFlux at any OpenAI-compatible or Anthropic endpoint \u2014 a self-hosted ' +
      'model, a company gateway, or a proxy. An API key is optional; the base URL is not.';
    customGroup.appendChild(customDesc);
    const customGrid = document.createElement('div');
    customGrid.className = 'llm-providers-grid';
    customGrid.id = 'llm-custom-grid';
    customGroup.appendChild(customGrid);
    section.appendChild(customGroup);

    // One document-level listener closes any open card menu on an outside
    // click. Registered here so it is installed exactly once with the section.
    document.addEventListener('click', () => {
      document.querySelectorAll('.llm-card-menu-popup').forEach((el) => el.remove());
    });

    return section;
  },

  // ── On-device inference ─────────────────────────────────
  //
  // Every decision about what to *say* lives in local-inference-logic.mjs,
  // which is unit-tested; this half only talks to the daemon and moves DOM.
  // The wire shapes are the daemon's own (`crates/daemon/src/local/`):
  // LocalState is tagged on `state`, LocalError on `code`, Fit on `fit`.

  _localLogic: null,
  _localStatus: null,
  /** Latch state, cached so the provider-switch guard can read it cheaply. */
  _localLatched: undefined,
  _localModels: null,
  _localSubscribed: false,

  _renderLocalGroup() {
    const group = this._createGroup('On-device');

    const desc = document.createElement('p');
    desc.className = 'section-desc';
    desc.textContent =
      'Run a model on this machine. Nothing leaves it — once on-device inference ' +
      'has answered, this process will not talk to a network model until it restarts.';
    group.appendChild(desc);

    const card = document.createElement('div');
    card.className = 'local-card';
    card.id = 'local-card';
    group.appendChild(card);

    const hint = document.createElement('p');
    hint.className = 'local-hint';
    hint.id = 'local-cuda-hint';
    hint.hidden = true;
    group.appendChild(hint);

    const config = document.createElement('div');
    config.className = 'local-config';
    config.id = 'local-config';
    config.hidden = true;
    group.appendChild(config);

    // Render is synchronous; the data arrives after.
    this._localRefresh().catch((e) => {
      card.textContent = `Could not read on-device status: ${e.message}`;
    });
    this._localEnsureSubscribed().catch((e) => {
      console.warn('[local] subscribe failed:', e);
    });

    return group;
  },

  /** Fetch `local.status` and repaint. */
  async _localRefresh() {
    const data = await this._retryWithBackoff(() =>
      this._sendAgentCommand('local.status', {})
    );
    this._localStatus = data || {};
    this._localLatched = !!(data && data.latched);
    await this._localPaint();
  },

  /**
   * The latch, for the provider-switch guard.
   *
   * Asks the daemon if the on-device group never rendered — someone can change
   * providers without ever opening it, and guessing `false` there would skip a
   * confirmation that exists precisely because the switch cannot take effect
   * until a restart.
   */
  async _localIsLatched() {
    if (this._localLatched === undefined) {
      try {
        const data = await this._sendAgentCommand('local.status', {});
        this._localLatched = !!(data && data.latched);
      } catch (_e) {
        this._localLatched = false;
      }
    }
    return this._localLatched;
  },

  async _localPaint() {
    const logic = await this._ensureLocalLogic();
    const card = document.getElementById('local-card');
    if (!card) return;

    const status = this._localStatus || {};
    // cardView cannot know which provider is answering; the daemon does not
    // report it (see the module's note on `activeProvider`).
    const active = this._llmProviders.find((p) => p.is_active);
    const view = logic.cardView({
      ...status,
      activeProvider: active ? active.display_name || active.id : null,
    });

    card.textContent = '';

    const head = document.createElement('div');
    head.className = 'local-card-head';
    const title = document.createElement('h3');
    title.className = 'local-card-title';
    title.textContent = view.title;
    const badge = document.createElement('span');
    badge.className = `local-badge local-badge-${view.badge.toLowerCase()}`;
    badge.textContent = view.badge;
    head.append(title, badge);
    card.appendChild(head);

    const sub = document.createElement('p');
    sub.className = 'local-card-sub';
    sub.textContent = view.subtitle;
    card.appendChild(sub);

    if (view.progress) {
      const wrap = document.createElement('div');
      wrap.className = 'local-progress';
      wrap.setAttribute('role', 'progressbar');
      wrap.setAttribute('aria-valuemin', '0');
      wrap.setAttribute('aria-valuemax', '100');
      wrap.setAttribute('aria-valuenow', String(view.progress.pct));
      const bar = document.createElement('div');
      bar.className = 'local-progress-bar';
      bar.style.width = `${view.progress.pct}%`;
      wrap.appendChild(bar);
      card.appendChild(wrap);
      const plabel = document.createElement('p');
      plabel.className = 'local-card-sub';
      plabel.textContent = view.progress.label;
      card.appendChild(plabel);
    }

    for (const [text, cls] of [
      [view.degradedNote, 'local-note-degraded'],
      [view.updateNote, 'local-note-update'],
    ]) {
      if (!text) continue;
      const note = document.createElement('p');
      note.className = `local-note ${cls}`;
      note.textContent = text;
      card.appendChild(note);
    }

    const actions = document.createElement('div');
    actions.className = 'local-actions';
    for (const action of view.actions) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = action.primary ? 'mcp-btn-primary' : 'mcp-btn-secondary';
      btn.textContent = action.label;
      btn.addEventListener('click', () => this._localAction(action.id));
      actions.appendChild(btn);
    }
    card.appendChild(actions);

    const statusLine = document.createElement('p');
    statusLine.className = 'local-card-sub';
    statusLine.id = 'local-action-status';
    card.appendChild(statusLine);

    await this._localPaintConfig();
  },

  /** Route a card button to its RPC. */
  async _localAction(id) {
    const say = (msg) => {
      const el = document.getElementById('local-action-status');
      if (el) el.textContent = msg;
    };
    try {
      switch (id) {
        case 'setup':
        case 'consent':
          await this._localBeginSetup();
          return;
        case 'cancel':
          await this._sendAgentCommand('local.cancel', {});
          break;
        case 'start':
        case 'retry':
          await this._sendAgentCommand('local.retry_backend', {});
          break;
        case 'stop':
          // Releasing the default is the only honest "stop": the latch holds
          // for the life of the process either way.
          say('On-device inference keeps this process offline until it restarts.');
          return;
        case 'update':
          await this._sendAgentCommand('local.update_engine', {});
          break;
        case 'repair':
          await this._sendAgentCommand('local.repair_engine', {});
          break;
        case 'set-default':
          await this._localSetDefault();
          return;
        default:
          return;
      }
      await this._localRefresh();
    } catch (e) {
      say(`Error: ${e.message}`);
    }
  },

  /** Probe → models → plan → consent → install. */
  async _localBeginSetup() {
    const say = (msg) => {
      const el = document.getElementById('local-action-status');
      if (el) el.textContent = msg;
    };
    say('Checking this machine…');
    const probe = await this._sendAgentCommand('local.probe', {});
    this._localApplyCudaHint(probe);

    this._localModels = await this._sendAgentCommand('local.models', {});
    const cfg = (this._localStatus && this._localStatus.config) || {};
    const model = this._localModels.find((m) => m.id === cfg.model) || this._localModels[0];
    if (!model) {
      say('No models available.');
      return;
    }
    const quant = (model.quants && model.quants[0]) || {};

    say('Working out the download…');
    const plan = await this._sendAgentCommand('local.plan', {
      model: model.id,
      quant: quant.bits,
    });
    say('');
    await this._localConsentModal(model, quant, plan);
  },

  /**
   * The consent dialog. Nothing is downloaded before this is accepted, which
   * is the whole point: several gigabytes should be a decision, not a
   * side effect of clicking "Set up".
   */
  async _localConsentModal(model, quant, plan) {
    const logic = await this._ensureLocalLogic();
    const summary = logic.consentSummary(plan);

    const modal = document.createElement('div');
    modal.className = 'llm-modal show';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'local-consent-title');

    const content = document.createElement('div');
    content.className = 'llm-modal-content';

    const header = document.createElement('div');
    header.className = 'llm-modal-header';
    const title = document.createElement('h2');
    title.id = 'local-consent-title';
    title.textContent = `Download ${model.display_name}?`;
    header.appendChild(title);
    const sub = document.createElement('p');
    sub.textContent = 'Nothing is downloaded until you accept.';
    header.appendChild(sub);
    content.appendChild(header);

    const list = document.createElement('ul');
    list.className = 'local-consent-list';
    for (const line of summary.lines) {
      const li = document.createElement('li');
      li.textContent = `${line.label} — ${logic.formatBytes(line.bytes)}`;
      list.appendChild(li);
    }
    const totalLi = document.createElement('li');
    totalLi.className = 'local-consent-total';
    totalLi.textContent = `Total — ${logic.formatBytes(summary.total)}`;
    list.appendChild(totalLi);
    content.appendChild(list);

    const from = document.createElement('p');
    from.className = 'local-card-sub';
    from.textContent = `From: ${summary.sources.join(', ')}`;
    content.appendChild(from);

    const disk = document.createElement('p');
    disk.className = summary.disk.enough ? 'local-card-sub' : 'llm-tos-warning';
    disk.textContent = summary.disk.enough
      ? `Needs ${logic.formatBytes(summary.disk.needed)}; ${logic.formatBytes(summary.disk.available)} free.`
      : `Needs ${logic.formatBytes(summary.disk.needed)} but only ${logic.formatBytes(summary.disk.available)} is free.`;
    content.appendChild(disk);

    const status = document.createElement('div');
    status.className = 'llm-modal-status';
    content.appendChild(status);

    const actions = document.createElement('div');
    actions.className = 'mcp-modal-actions';

    const cancelBtn = document.createElement('button');
    cancelBtn.type = 'button';
    cancelBtn.className = 'mcp-btn-secondary';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.addEventListener('click', () => modal.remove());
    actions.appendChild(cancelBtn);

    if (summary.altButton) {
      const altBtn = document.createElement('button');
      altBtn.type = 'button';
      altBtn.className = 'mcp-btn-secondary';
      altBtn.textContent = summary.altButton.label;
      altBtn.addEventListener('click', async () => {
        altBtn.disabled = true;
        try {
          const replanned = await this._sendAgentCommand('local.plan', {
            model: model.id,
            quant: quant.bits,
            backend: summary.altButton.backend,
          });
          modal.remove();
          await this._localConsentModal(model, quant, replanned);
        } catch (e) {
          status.textContent = `Error: ${e.message}`;
          status.className = 'llm-modal-status error';
          altBtn.disabled = false;
        }
      });
      actions.appendChild(altBtn);
    }

    const okBtn = document.createElement('button');
    okBtn.type = 'button';
    okBtn.className = 'mcp-btn-primary';
    okBtn.textContent = 'Download';
    okBtn.disabled = !summary.disk.enough;
    okBtn.addEventListener('click', async () => {
      okBtn.disabled = true;
      status.textContent = 'Starting…';
      try {
        await this._sendAgentCommand('local.install', {
          model: model.id,
          quant: quant.bits,
          backend: plan.backend,
        });
        modal.remove();
        await this._localRefresh();
      } catch (e) {
        status.textContent = `Error: ${e.message}`;
        status.className = 'llm-modal-status error';
        okBtn.disabled = false;
      }
    });
    actions.appendChild(okBtn);

    content.appendChild(actions);
    modal.appendChild(content);
    modal.addEventListener('click', (e) => {
      if (e.target === modal) modal.remove();
    });
    document.body.appendChild(modal);
    setTimeout(() => cancelBtn.focus(), 50);
  },

  /** Make on-device the default, and report what the latch paused. */
  async _localSetDefault() {
    const el = document.getElementById('local-action-status');
    const say = (msg) => {
      if (el) el.textContent = msg;
    };
    say('Switching…');
    const res = await this._sendAgentCommand('local.set_default', { on: true });
    const paused = res || {};
    const bits = [
      [paused.paused_loops, 'loop'],
      [paused.paused_schedules, 'schedule'],
      [paused.paused_goals, 'goal'],
    ]
      .filter(([n]) => Number(n) > 0)
      .map(([n, word]) => `${n} ${word}${Number(n) === 1 ? '' : 's'}`);
    say(
      bits.length
        ? `On-device is now the default. Paused: ${bits.join(', ')}.`
        : 'On-device is now the default.'
    );
    await this._localRefresh();
    await this._populateLlmProviders();
  },

  /**
   * Confirm leaving on-device while the latch is on.
   *
   * Resolves true to proceed. The switch cannot take effect until a restart,
   * so saying nothing would look like the setting simply failed to save.
   */
  async _confirmExitLocal(providerName) {
    return new Promise((resolve) => {
      const modal = document.createElement('div');
      modal.className = 'llm-modal show';
      modal.setAttribute('role', 'dialog');
      modal.setAttribute('aria-modal', 'true');
      modal.setAttribute('aria-labelledby', 'local-exit-title');

      const content = document.createElement('div');
      content.className = 'llm-modal-content';

      const header = document.createElement('div');
      header.className = 'llm-modal-header';
      const title = document.createElement('h2');
      title.id = 'local-exit-title';
      title.textContent = `Switch to ${providerName}?`;
      header.appendChild(title);
      content.appendChild(header);

      const warn = document.createElement('div');
      warn.className = 'llm-tos-warning';
      warn.textContent =
        'On-device inference has already answered in this session, so this ' +
        'process stays offline until NevoFlux restarts. The change is saved ' +
        'now and takes effect after a restart.';
      content.appendChild(warn);

      const actions = document.createElement('div');
      actions.className = 'mcp-modal-actions';
      const cancelBtn = document.createElement('button');
      cancelBtn.type = 'button';
      cancelBtn.className = 'mcp-btn-secondary';
      cancelBtn.textContent = 'Keep on-device';
      cancelBtn.addEventListener('click', () => {
        modal.remove();
        resolve(false);
      });
      const okBtn = document.createElement('button');
      okBtn.type = 'button';
      okBtn.className = 'mcp-btn-primary';
      okBtn.textContent = 'Switch anyway';
      okBtn.addEventListener('click', () => {
        modal.remove();
        resolve(true);
      });
      actions.append(cancelBtn, okBtn);
      content.appendChild(actions);

      modal.appendChild(content);
      document.body.appendChild(modal);
      setTimeout(() => cancelBtn.focus(), 50);
    });
  },

  /** NVIDIA hardware present but no usable CUDA runtime. */
  _localApplyCudaHint(probe) {
    const hint = document.getElementById('local-cuda-hint');
    if (!hint || !probe) return;
    const stranded = probe.has_physical_nvidia && !probe.has_usable_nvidia;
    hint.hidden = !stranded;
    if (stranded) {
      hint.textContent =
        'An NVIDIA GPU is present but no usable CUDA runtime was found. ' +
        'Install the CUDA runtime to use it, or continue on Vulkan or CPU.';
    }
  },

  async _localPaintConfig() {
    const box = document.getElementById('local-config');
    if (!box) return;
    const status = this._localStatus || {};
    const cfg = status.config || {};
    const state = status.state && status.state.state;
    // Only worth showing once something is installed.
    box.hidden = state === 'idle' || state === undefined;
    if (box.hidden) return;

    box.textContent = '';
    const logic = await this._ensureLocalLogic();

    if (!this._localModels) {
      try {
        this._localModels = await this._sendAgentCommand('local.models', {});
      } catch (_e) {
        this._localModels = [];
      }
    }

    const modelSel = document.createElement('select');
    for (const m of this._localModels) {
      const q = (m.quants && m.quants[0]) || {};
      const opt = logic.modelOptionLabel(m, q);
      const el = document.createElement('option');
      el.value = m.id;
      el.textContent = opt.text;
      el.disabled = opt.disabled;
      el.selected = m.id === cfg.model;
      modelSel.appendChild(el);
    }
    modelSel.addEventListener('change', () =>
      this._localSetConfig({ model: modelSel.value })
    );
    box.appendChild(this._localRow('Model', modelSel));

    // macOS forces Metal; a backend choice there would be a control that does
    // nothing. `local.status` carries no probe, so the platform comes from the
    // page, the same way the KB section decides it.
    const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
    if (!isMac) {
      const backendSel = document.createElement('select');
      for (const b of ['auto', 'cpu', 'vulkan', 'cuda']) {
        const el = document.createElement('option');
        el.value = b;
        el.textContent = b === 'auto' ? 'Automatic' : b.toUpperCase();
        el.selected = (cfg.backend || 'auto') === b;
        backendSel.appendChild(el);
      }
      backendSel.addEventListener('change', () =>
        this._localSetConfig({ backend: backendSel.value })
      );
      box.appendChild(this._localRow('Backend', backendSel));
    }

    const ctxSel = document.createElement('select');
    for (const [value, label] of [
      ['auto', 'Automatic'],
      ['16384', '16K'],
      ['32768', '32K'],
    ]) {
      const el = document.createElement('option');
      el.value = value;
      el.textContent = label;
      el.selected = String(cfg.ctx_size ?? 'auto') === value;
      ctxSel.appendChild(el);
    }
    ctxSel.addEventListener('change', () =>
      this._localSetConfig({
        ctx_size: ctxSel.value === 'auto' ? 'auto' : Number(ctxSel.value),
      })
    );
    box.appendChild(this._localRow('Context', ctxSel));

    if (status.engine) {
      const line = document.createElement('p');
      line.className = 'local-card-sub';
      line.textContent = `Engine ${status.engine.tag} · ${status.engine.kind} · ${logic.formatBytes(status.engine.bytes)}`;
      box.appendChild(line);
    }
  },

  _localRow(label, control) {
    const row = document.createElement('div');
    row.className = 'form-row';
    const lbl = document.createElement('label');
    lbl.textContent = label;
    row.append(lbl, control);
    return row;
  },

  async _localSetConfig(params) {
    const el = document.getElementById('local-action-status');
    try {
      await this._sendAgentCommand('local.set_config', params);
      await this._localRefresh();
    } catch (e) {
      if (el) el.textContent = `Could not save: ${e.message}`;
    }
  },

  /**
   * Subscribe to the three on-device topics.
   *
   * Two-step, like the speech models and the KB wizard: a persistent EventBus
   * channel, then subscribe through the bridge. State and latch are sticky and
   * replayed, so a page opened mid-download still paints the right card.
   */
  async _localEnsureSubscribed() {
    if (this._localSubscribed) return;
    this._localSubscribed = true;

    const channelId = 'local_' + Date.now() + '_' + Math.random().toString(36).slice(2, 8);
    await NevofluxPage.sendQuery('events:channel_open', { channelId });

    const listener = (event) => {
      const detail = event.detail;
      if (!detail || detail.type !== 'bridge:push') return;
      const msg = detail.msg;
      if (!msg || msg.type !== 'events:delivery') return;
      const ev = msg.payload?.event;
      if (!ev) return;
      try {
        if (ev.topic === 'system:local:state') {
          this._localStatus = { ...(this._localStatus || {}), state: ev.payload };
          this._localPaint();
        } else if (ev.topic === 'system:local:latch_changed') {
          this._localLatched = !!(ev.payload && ev.payload.on);
        } else if (ev.topic === 'system:local:progress') {
          this._localStatus = { ...(this._localStatus || {}), state: ev.payload };
          this._localPaint();
        }
      } catch (err) {
        console.warn('[local] event handler failed:', err);
      }
    };
    window.addEventListener('NevofluxMessage', listener);

    const res = await NevofluxPage.sendQuery('bridge:request', {
      type: 'events.subscribe',
      payload: {
        patterns: ['system:local:state', 'system:local:progress', 'system:local:latch_changed'],
        replay_sticky: true,
        channel_id: channelId,
      },
    });
    if (!res || res.success === false) {
      window.removeEventListener('NevofluxMessage', listener);
      this._localSubscribed = false;
      try {
        await NevofluxPage.sendQuery('events:channel_close', { channelId });
      } catch (_e) {}
      throw new Error(res?.error?.message || 'events.subscribe failed');
    }
  },

  async _populateLlmProviders() {
    const cache = await this._getStatusCache();
    const isFirstLaunch = !cache;

    if (isFirstLaunch) {
      await this._populateLlmFirstLaunch();
    } else {
      await this._populateLlmSubsequentLaunch();
    }
  },

  async _populateLlmFirstLaunch() {
    this._showLlmConnecting('NevoFlux Agent is starting, please wait...');

    try {
      const status = await this._retryWithBackoff(() => this._queryAgentStatus());
      if (status.first_run) {
        this._showGuidanceBanner(
          'Select an AI provider and configure your API Key to get started, or complete quick setup in the sidebar (Ctrl+Shift+A)'
        );
      }
      await this._loadAndRenderProviders();
    } catch (e) {
      this._showLlmError(
        'Agent failed to start. Please check your installation.',
        () => this._populateLlmProviders()
      );
    }
  },

  async _populateLlmSubsequentLaunch() {
    this._showLlmSkeleton();

    const startTime = Date.now();
    const pollInterval = 2000;
    const timeout = 30000;

    const poll = async () => {
      try {
        await this._queryAgentStatus();
        await this._loadAndRenderProviders();
      } catch (e) {
        if (Date.now() - startTime < timeout) {
          setTimeout(poll, pollInterval);
        } else {
          this._showLlmError(
            'Connection timeout. Agent may not be running.',
            () => this._populateLlmProviders()
          );
        }
      }
    };

    poll();
  },

  async _loadAndRenderProviders() {
    await this._ensureCustomLogic();
    try {
      const data = await this._sendAgentCommand('config.llm.list');
      this._llmProviders = data?.providers || [];
    } catch (e) {
      console.warn('Failed to load LLM providers:', e);
      this._llmProviders = [];
    }
    this._refreshLlmGrid();
  },

  _refreshLlmGrid() {
    const llmGrid = document.getElementById('llm-providers-grid');
    const agentGrid = document.getElementById('llm-agents-grid');
    const customGrid = document.getElementById('llm-custom-grid');
    if (!llmGrid) return;
    llmGrid.innerHTML = '';
    if (agentGrid) agentGrid.innerHTML = '';
    if (customGrid) customGrid.innerHTML = '';

    // The add-card is always available, even before any provider exists.
    if (customGrid) customGrid.appendChild(this._createAddCustomCard());

    const providers = this._llmProviders;
    if (!providers.length) {
      const empty = document.createElement('div');
      empty.className = 'llm-loading';
      empty.textContent = 'No providers available.';
      llmGrid.appendChild(empty);
      return;
    }

    const route = this._customLogic?.routeProviderToGrid;
    for (const provider of providers) {
      const target = route ? route(provider) : 'llm';
      if (target === 'custom' && customGrid) {
        customGrid.appendChild(this._createCustomProviderCard(provider));
      } else if (target === 'agents' && agentGrid) {
        agentGrid.appendChild(this._createProviderCard(provider));
      } else {
        llmGrid.appendChild(this._createProviderCard(provider));
      }
    }
  },

  _createProviderCard(provider) {
    const card = document.createElement('div');
    card.className = 'llm-provider-card';
    if (provider.configured) card.classList.add('configured');
    if (provider.active) card.classList.add('active');

    // Icon
    const iconWrap = document.createElement('div');
    iconWrap.className = 'llm-provider-icon';
    if (provider.icon) {
      const img = document.createElement('img');
      img.src = provider.icon;
      img.alt = provider.display_name || provider.id;
      iconWrap.appendChild(img);
    } else {
      iconWrap.textContent = (provider.display_name || provider.id)[0].toUpperCase();
    }
    card.appendChild(iconWrap);

    // Info
    const info = document.createElement('div');
    info.className = 'llm-provider-info';

    const name = document.createElement('div');
    name.className = 'llm-provider-name';
    name.textContent = provider.display_name || provider.id;
    info.appendChild(name);

    const type = document.createElement('div');
    type.className = 'llm-provider-type';
    type.textContent = provider.type || 'service';
    info.appendChild(type);

    card.appendChild(info);

    // Status indicators
    const status = document.createElement('div');
    status.className = 'llm-provider-status';

    if (provider.active) {
      const activeBadge = document.createElement('span');
      activeBadge.className = 'llm-badge llm-badge-active';
      activeBadge.textContent = 'Active';
      status.appendChild(activeBadge);
    }
    if (provider.configured) {
      const checkmark = document.createElement('span');
      checkmark.className = 'llm-checkmark';
      checkmark.textContent = '\u2713';
      status.appendChild(checkmark);
    }
    card.appendChild(status);

    card.addEventListener('click', () => this._showProviderModal(provider));
    return card;
  },

  /** The dashed "+ Add Custom Provider" tile that opens an empty modal. */
  _createAddCustomCard() {
    const card = document.createElement('div');
    card.className = 'llm-provider-card llm-add-card';
    card.id = 'llm-add-custom-card';
    card.setAttribute('role', 'button');
    card.setAttribute('tabindex', '0');
    card.setAttribute('aria-label', 'Add a custom provider');

    const icon = document.createElement('div');
    icon.className = 'llm-provider-icon llm-add-icon';
    icon.textContent = '+';
    card.appendChild(icon);

    const info = document.createElement('div');
    info.className = 'llm-provider-info';
    const name = document.createElement('div');
    name.className = 'llm-provider-name';
    name.textContent = 'Add Custom Provider';
    info.appendChild(name);
    card.appendChild(info);

    const open = () => this._showCustomProviderModal(null);
    card.addEventListener('click', open);
    card.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        open();
      }
    });
    return card;
  },

  /** A user-defined provider card: initial on its accent, wire label, menu. */
  _createCustomProviderCard(provider) {
    const logic = this._customLogic;
    const card = document.createElement('div');
    card.className = 'llm-provider-card llm-custom-card';
    if (provider.configured) card.classList.add('configured');
    if (provider.active) card.classList.add('active');

    const iconWrap = document.createElement('div');
    iconWrap.className = 'llm-provider-icon';
    if (provider.accent) {
      iconWrap.style.background = provider.accent;
      iconWrap.style.color = '#fff';
    }
    iconWrap.textContent = logic
      ? logic.providerInitial(provider.display_name || provider.id)
      : '?';
    card.appendChild(iconWrap);

    const info = document.createElement('div');
    info.className = 'llm-provider-info';
    const name = document.createElement('div');
    name.className = 'llm-provider-name';
    name.textContent = provider.display_name || provider.id;
    info.appendChild(name);
    const type = document.createElement('div');
    type.className = 'llm-provider-type';
    type.textContent = logic ? logic.wireLabel(provider.wire) : provider.wire || 'custom';
    info.appendChild(type);
    card.appendChild(info);

    const status = document.createElement('div');
    status.className = 'llm-provider-status';
    if (provider.active) {
      const activeBadge = document.createElement('span');
      activeBadge.className = 'llm-badge llm-badge-active';
      activeBadge.textContent = 'Active';
      status.appendChild(activeBadge);
    }
    if (provider.configured) {
      const checkmark = document.createElement('span');
      checkmark.className = 'llm-checkmark';
      checkmark.textContent = '\u2713';
      status.appendChild(checkmark);
    }
    card.appendChild(status);

    // Corner menu. Kept out of the card's own click target.
    const menuBtn = document.createElement('button');
    menuBtn.className = 'llm-card-menu';
    menuBtn.type = 'button';
    menuBtn.textContent = '\u22ef';
    menuBtn.setAttribute(
      'aria-label',
      `Actions for ${provider.display_name || provider.id}`
    );
    menuBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      this._toggleCustomCardMenu(card, provider);
    });
    card.appendChild(menuBtn);

    card.addEventListener('click', () => this._showCustomProviderModal(provider));
    return card;
  },

  /** Open/close the per-card Edit/Delete menu. */
  _toggleCustomCardMenu(card, provider) {
    const existing = card.querySelector('.llm-card-menu-popup');
    document.querySelectorAll('.llm-card-menu-popup').forEach((el) => el.remove());
    if (existing) return;

    const popup = document.createElement('div');
    popup.className = 'llm-card-menu-popup';
    popup.addEventListener('click', (e) => e.stopPropagation());

    const edit = document.createElement('button');
    edit.type = 'button';
    edit.className = 'llm-card-menu-item';
    edit.textContent = 'Edit';
    edit.addEventListener('click', (e) => {
      e.stopPropagation();
      popup.remove();
      this._showCustomProviderModal(provider);
    });
    popup.appendChild(edit);

    const del = document.createElement('button');
    del.type = 'button';
    del.className = 'llm-card-menu-item llm-card-menu-danger';
    del.textContent = 'Delete';
    del.addEventListener('click', (e) => {
      e.stopPropagation();
      popup.remove();
      this._confirmDeleteCustomProvider(provider);
    });
    popup.appendChild(del);

    card.appendChild(popup);
  },

  /**
   * Open the shared LLM modal in custom-provider mode.
   *
   * `provider` is a config.llm.list entry when editing, or null when creating.
   * `_llmEditCustomId` is the mode flag the save button reads: `undefined`
   * means "builtin provider", `null` means "creating a custom one", a string
   * means "editing that custom one".
   */
  async _showCustomProviderModal(provider) {
    await this._ensureCustomLogic();
    this._ensureLlmModal();
    this._llmEditProvider = null;
    this._llmEditCustomId = provider ? provider.id : null;

    document.getElementById('llm-modal-title').textContent = provider
      ? `Edit ${provider.display_name || provider.id}`
      : 'Add Custom Provider';
    document.getElementById('llm-modal-subtitle').textContent =
      'Any OpenAI-compatible or Anthropic endpoint. API key optional.';

    // Builtin-only blocks off, custom block on.
    document.getElementById('llm-modal-tos-warning').style.display = 'none';
    document.getElementById('llm-modal-openclaw-fields').style.display = 'none';
    document.getElementById('llm-modal-custom-fields').style.display = '';
    document.getElementById('llm-modal-baseurl-group').style.display = '';

    const status = document.getElementById('llm-modal-status');
    status.textContent = '';
    status.className = 'llm-modal-status';

    const wire = provider?.wire || 'openai';
    document.getElementById('llm-modal-custom-name').value = provider?.display_name || '';
    document.getElementById('llm-modal-custom-wire').value = wire;
    document.getElementById('llm-modal-custom-context').value = provider?.context_window ?? '';
    document.getElementById('llm-modal-custom-streaming').checked =
      provider?.use_streaming !== false;
    document.getElementById('llm-modal-model').value = provider?.model || '';
    document.getElementById('llm-modal-baseurl').value = provider?.base_url || '';
    document.getElementById('llm-modal-set-active').checked = !!provider?.active;
    document.getElementById('llm-modal-apikey').value = '';

    this._syncCustomWireHints(wire);

    // Accent selection
    const swatches = document.getElementById('llm-modal-custom-accent');
    swatches.querySelectorAll('.llm-accent-swatch').forEach((el) => {
      el.classList.toggle('selected', (el.dataset.accent || '') === (provider?.accent || ''));
    });

    document.getElementById('llm-modal-baseurl-help').textContent =
      'Required. Include the version path, e.g. https://host/v1';

    // Reflect whether a key is already stored.
    if (provider) {
      try {
        const data = await this._sendAgentCommand('config.llm.get', { provider: provider.id });
        document.getElementById('llm-modal-apikey').placeholder = data?.has_api_key
          ? `Current: ${data.api_key || 'configured'}`
          : 'Optional \u2014 leave empty for a keyless endpoint';
        document.getElementById('llm-modal-key-help').textContent = data?.has_api_key
          ? 'Key is configured. Leave blank to keep current.'
          : 'Optional. Local servers usually need no key.';
      } catch (e) {
        console.warn('Failed to load custom provider config:', e);
      }
    } else {
      document.getElementById('llm-modal-apikey').placeholder =
        'Optional \u2014 leave empty for a keyless endpoint';
      document.getElementById('llm-modal-key-help').textContent =
        'Optional. Local servers usually need no key.';
    }

    this._llmModal.classList.add('show');
    setTimeout(() => document.getElementById('llm-modal-custom-name').focus(), 50);
  },

  /** Update the wire-dependent placeholders and defaults. */
  _syncCustomWireHints(wire) {
    const isAnthropic = wire === 'anthropic';
    document.getElementById('llm-modal-model').placeholder = isAnthropic
      ? 'e.g. claude-sonnet-4-20250514'
      : 'e.g. gpt-4o';
    document.getElementById('llm-modal-baseurl').placeholder = isAnthropic
      ? 'https://host'
      : 'https://host/v1';
    document.getElementById('llm-modal-custom-context').placeholder = isAnthropic
      ? '200000'
      : '128000';
    document.getElementById('llm-modal-custom-context-help').textContent = isAnthropic
      ? 'Default: 200000. Sizing for the context compressor.'
      : 'Default: 128000. Sizing for the context compressor.';
  },

  /** Create or update a custom provider from the modal's fields. */
  async _saveCustomProvider() {
    const logic = await this._ensureCustomLogic();
    const statusEl = document.getElementById('llm-modal-status');
    const saveBtn = document.getElementById('llm-modal-save');

    const selectedSwatch = document
      .getElementById('llm-modal-custom-accent')
      .querySelector('.llm-accent-swatch.selected');

    const form = {
      displayName: document.getElementById('llm-modal-custom-name').value,
      wire: document.getElementById('llm-modal-custom-wire').value,
      apiKey: document.getElementById('llm-modal-apikey').value,
      model: document.getElementById('llm-modal-model').value,
      baseUrl: document.getElementById('llm-modal-baseurl').value,
      contextWindow: document.getElementById('llm-modal-custom-context').value,
      useStreaming: document.getElementById('llm-modal-custom-streaming').checked,
      accent: selectedSwatch ? selectedSwatch.dataset.accent : '',
      setActive: document.getElementById('llm-modal-set-active').checked,
    };

    const { ok, errors } = logic.validateCustomForm(form);
    if (!ok) {
      statusEl.textContent = Object.values(errors)[0];
      statusEl.className = 'llm-modal-status error';
      return;
    }

    const isCreate = this._llmEditCustomId === null;
    const params = logic.buildCustomParams(form, {
      isCreate,
      id: this._llmEditCustomId,
    });

    // A custom provider activates through `set_active` exactly like a built-in
    // one, so it needs the same confirmation. The plan only pointed at the
    // built-in path; guarding just that one would let this switch away from
    // on-device with no warning at all.
    if (params.set_active && (await this._localIsLatched())) {
      const localLogic = await this._ensureLocalLogic();
      const target = params.id || 'custom';
      if (localLogic.needsExitConfirm(true, target)) {
        const go = await this._confirmExitLocal(params.display_name || 'this provider');
        if (!go) {
          statusEl.textContent = 'Kept on-device.';
          statusEl.className = 'llm-modal-status';
          return;
        }
      }
    }

    saveBtn.disabled = true;
    statusEl.textContent = 'Saving...';
    statusEl.className = 'llm-modal-status';

    try {
      await this._sendAgentCommand(
        isCreate ? 'config.llm.custom.create' : 'config.llm.custom.update',
        params
      );
      statusEl.textContent = 'Saved successfully!';
      statusEl.className = 'llm-modal-status success';
      await this._populateLlmProviders();
      setTimeout(() => this._closeLlmModal(), 600);
    } catch (e) {
      statusEl.textContent = `Error: ${e.message}`;
      statusEl.className = 'llm-modal-status error';
    } finally {
      saveBtn.disabled = false;
    }
  },

  /**
   * Confirm and delete a custom provider.
   *
   * When the target is active the dialog names the provider that will take
   * over, computed the same way the daemon computes it.
   */
  async _confirmDeleteCustomProvider(provider) {
    const logic = await this._ensureCustomLogic();
    const warning = logic.deleteWarning(provider, this._llmProviders);

    const modal = document.createElement('div');
    modal.className = 'llm-modal show';
    modal.addEventListener('click', (e) => {
      if (e.target === modal) modal.remove();
    });

    const content = document.createElement('div');
    content.className = 'llm-modal-content';

    const header = document.createElement('div');
    header.className = 'llm-modal-header';
    const title = document.createElement('h2');
    title.textContent = `Delete "${provider.display_name || provider.id}"?`;
    header.appendChild(title);
    content.appendChild(header);

    if (warning.active) {
      const warn = document.createElement('div');
      warn.className = 'llm-tos-warning';
      warn.textContent = warning.fallbackName
        ? `It is the active provider. Deleting it switches to: ${warning.fallbackName}. ` +
          'Its API key is removed from config.toml as well.'
        : 'It is the active provider and nothing else is configured. Deleting it ' +
          'leaves no active provider \u2014 the sidebar returns to setup.';
      content.appendChild(warn);
    }

    const status = document.createElement('div');
    status.className = 'llm-modal-status';
    content.appendChild(status);

    const actions = document.createElement('div');
    actions.className = 'mcp-modal-actions';
    const cancelBtn = document.createElement('button');
    cancelBtn.className = 'mcp-btn-secondary';
    cancelBtn.type = 'button';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.addEventListener('click', () => modal.remove());
    const deleteBtn = document.createElement('button');
    deleteBtn.className = 'mcp-btn-primary llm-btn-danger';
    deleteBtn.type = 'button';
    deleteBtn.textContent = 'Delete';
    deleteBtn.addEventListener('click', async () => {
      deleteBtn.disabled = true;
      status.textContent = 'Deleting...';
      status.className = 'llm-modal-status';
      try {
        await this._sendAgentCommand('config.llm.custom.delete', { id: provider.id });
        modal.remove();
        await this._populateLlmProviders();
      } catch (e) {
        status.textContent = `Error: ${e.message}`;
        status.className = 'llm-modal-status error';
        deleteBtn.disabled = false;
      }
    });
    actions.appendChild(cancelBtn);
    actions.appendChild(deleteBtn);
    content.appendChild(actions);

    modal.appendChild(content);
    document.body.appendChild(modal);
    setTimeout(() => cancelBtn.focus(), 50);
  },

  // ── LLM Provider Modal ─────────────────────────────────

  _ensureLlmModal() {
    if (this._llmModal) return;

    const modal = document.createElement('div');
    modal.className = 'llm-modal';
    modal.id = 'llm-modal';
    modal.addEventListener('click', (e) => {
      if (e.target === modal) this._closeLlmModal();
    });

    const content = document.createElement('div');
    content.className = 'llm-modal-content';

    // Header
    const header = document.createElement('div');
    header.className = 'llm-modal-header';
    const title = document.createElement('h2');
    title.id = 'llm-modal-title';
    title.textContent = 'Configure Provider';
    const subtitle = document.createElement('p');
    subtitle.id = 'llm-modal-subtitle';
    subtitle.textContent = '';
    header.appendChild(title);
    header.appendChild(subtitle);
    content.appendChild(header);

    // Form
    const form = document.createElement('div');
    form.id = 'llm-modal-form';

    // ToS warning (shown only for providers that require one, e.g. antigravity)
    const tosWarn = document.createElement('div');
    tosWarn.className = 'llm-tos-warning';
    tosWarn.id = 'llm-modal-tos-warning';
    tosWarn.style.display = 'none';
    tosWarn.textContent =
      "Unofficial adapter (antigravity-acp). Using a personal Google account with third-party Antigravity access may violate Google's Terms of Service and risk account suspension. Requires 'antigravity-acp' on PATH.";
    form.appendChild(tosWarn);

    // API Key
    const keyGroup = document.createElement('div');
    keyGroup.className = 'mcp-form-group';
    const keyLabel = document.createElement('label');
    keyLabel.className = 'mcp-form-label';
    keyLabel.textContent = 'API Key';
    keyGroup.appendChild(keyLabel);

    const keyWrapper = document.createElement('div');
    keyWrapper.className = 'password-wrapper';
    const keyInput = document.createElement('input');
    keyInput.className = 'mcp-form-input';
    keyInput.type = 'password';
    keyInput.id = 'llm-modal-apikey';
    keyInput.placeholder = 'Enter API key...';
    const keyToggle = document.createElement('button');
    keyToggle.className = 'password-toggle';
    keyToggle.type = 'button';
    keyToggle.textContent = 'Show';
    keyToggle.addEventListener('click', () => {
      if (keyInput.type === 'password') {
        keyInput.type = 'text';
        keyToggle.textContent = 'Hide';
      } else {
        keyInput.type = 'password';
        keyToggle.textContent = 'Show';
      }
    });
    keyWrapper.appendChild(keyInput);
    keyWrapper.appendChild(keyToggle);
    keyGroup.appendChild(keyWrapper);

    const keyHelp = document.createElement('div');
    keyHelp.className = 'mcp-form-help';
    keyHelp.id = 'llm-modal-key-help';
    keyHelp.textContent = '';
    keyGroup.appendChild(keyHelp);
    form.appendChild(keyGroup);

    // Model
    const modelGroup = document.createElement('div');
    modelGroup.className = 'mcp-form-group';
    const modelLabel = document.createElement('label');
    modelLabel.className = 'mcp-form-label';
    modelLabel.textContent = 'Model';
    modelGroup.appendChild(modelLabel);
    const modelInput = document.createElement('input');
    modelInput.className = 'mcp-form-input';
    modelInput.type = 'text';
    modelInput.id = 'llm-modal-model';
    modelInput.placeholder = 'Leave empty for default';
    modelGroup.appendChild(modelInput);
    const modelHelp = document.createElement('div');
    modelHelp.className = 'mcp-form-help';
    modelHelp.id = 'llm-modal-model-help';
    modelHelp.textContent = '';
    modelGroup.appendChild(modelHelp);
    form.appendChild(modelGroup);

    // Base URL (hidden for CLI providers)
    const baseUrlGroup = document.createElement('div');
    baseUrlGroup.className = 'mcp-form-group';
    baseUrlGroup.id = 'llm-modal-baseurl-group';
    const baseUrlLabel = document.createElement('label');
    baseUrlLabel.className = 'mcp-form-label';
    baseUrlLabel.textContent = 'Base URL';
    baseUrlGroup.appendChild(baseUrlLabel);
    const baseUrlInput = document.createElement('input');
    baseUrlInput.className = 'mcp-form-input';
    baseUrlInput.type = 'text';
    baseUrlInput.id = 'llm-modal-baseurl';
    baseUrlInput.placeholder = 'Leave empty for default endpoint';
    baseUrlGroup.appendChild(baseUrlInput);
    const baseUrlHelp = document.createElement('div');
    baseUrlHelp.className = 'mcp-form-help';
    baseUrlHelp.id = 'llm-modal-baseurl-help';
    baseUrlHelp.textContent = 'Custom API endpoint for proxy or compatible services.';
    baseUrlGroup.appendChild(baseUrlHelp);
    form.appendChild(baseUrlGroup);

    // === OpenClaw-specific fields (hidden for other providers) ===
    const openclawFields = document.createElement('div');
    openclawFields.id = 'llm-modal-openclaw-fields';
    openclawFields.style.display = 'none';

    // Provider Name
    const provNameGroup = document.createElement('div');
    provNameGroup.className = 'mcp-form-group';
    const provNameLabel = document.createElement('label');
    provNameLabel.className = 'mcp-form-label';
    provNameLabel.textContent = 'Provider Name';
    provNameGroup.appendChild(provNameLabel);
    const provNameInput = document.createElement('input');
    provNameInput.className = 'mcp-form-input';
    provNameInput.type = 'text';
    provNameInput.id = 'llm-modal-oc-provider-name';
    provNameInput.placeholder = 'openai';
    provNameGroup.appendChild(provNameInput);
    openclawFields.appendChild(provNameGroup);

    // API Type
    const apiTypeGroup = document.createElement('div');
    apiTypeGroup.className = 'mcp-form-group';
    const apiTypeLabel = document.createElement('label');
    apiTypeLabel.className = 'mcp-form-label';
    apiTypeLabel.textContent = 'API Type';
    apiTypeGroup.appendChild(apiTypeLabel);
    const apiTypeSelect = document.createElement('select');
    apiTypeSelect.className = 'mcp-form-input';
    apiTypeSelect.id = 'llm-modal-oc-api-type';
    for (const opt of ['openai-completions', 'anthropic', 'openai-responses']) {
      const o = document.createElement('option');
      o.value = opt;
      o.textContent = opt;
      apiTypeSelect.appendChild(o);
    }
    apiTypeGroup.appendChild(apiTypeSelect);
    openclawFields.appendChild(apiTypeGroup);

    // Context Window
    const ctxGroup = document.createElement('div');
    ctxGroup.className = 'mcp-form-group';
    const ctxLabel = document.createElement('label');
    ctxLabel.className = 'mcp-form-label';
    ctxLabel.textContent = 'Context Window';
    ctxGroup.appendChild(ctxLabel);
    const ctxInput = document.createElement('input');
    ctxInput.className = 'mcp-form-input';
    ctxInput.type = 'number';
    ctxInput.id = 'llm-modal-oc-context-window';
    ctxInput.placeholder = '92160';
    ctxGroup.appendChild(ctxInput);
    openclawFields.appendChild(ctxGroup);

    // Max Tokens
    const maxTokGroup = document.createElement('div');
    maxTokGroup.className = 'mcp-form-group';
    const maxTokLabel = document.createElement('label');
    maxTokLabel.className = 'mcp-form-label';
    maxTokLabel.textContent = 'Max Tokens';
    maxTokGroup.appendChild(maxTokLabel);
    const maxTokInput = document.createElement('input');
    maxTokInput.className = 'mcp-form-input';
    maxTokInput.type = 'number';
    maxTokInput.id = 'llm-modal-oc-max-tokens';
    maxTokInput.placeholder = '32768';
    maxTokGroup.appendChild(maxTokInput);
    openclawFields.appendChild(maxTokGroup);

    // Reasoning checkbox
    const reasonGroup = document.createElement('div');
    reasonGroup.className = 'mcp-form-group llm-active-group';
    const reasonLabel = document.createElement('label');
    reasonLabel.className = 'llm-active-label';
    const reasonCheckbox = document.createElement('input');
    reasonCheckbox.type = 'checkbox';
    reasonCheckbox.id = 'llm-modal-oc-reasoning';
    const reasonText = document.createElement('span');
    reasonText.textContent = 'Supports reasoning';
    reasonLabel.appendChild(reasonCheckbox);
    reasonLabel.appendChild(reasonText);
    reasonGroup.appendChild(reasonLabel);
    openclawFields.appendChild(reasonGroup);

    form.appendChild(openclawFields);

    // === Custom-provider fields (hidden for builtin providers) ===
    const customFields = document.createElement('div');
    customFields.id = 'llm-modal-custom-fields';
    customFields.style.display = 'none';

    // Display name
    const cnGroup = document.createElement('div');
    cnGroup.className = 'mcp-form-group';
    const cnLabel = document.createElement('label');
    cnLabel.className = 'mcp-form-label';
    cnLabel.textContent = 'Name';
    cnLabel.setAttribute('for', 'llm-modal-custom-name');
    cnGroup.appendChild(cnLabel);
    const cnInput = document.createElement('input');
    cnInput.className = 'mcp-form-input';
    cnInput.type = 'text';
    cnInput.id = 'llm-modal-custom-name';
    cnInput.placeholder = 'My LLM';
    cnGroup.appendChild(cnInput);
    const cnHelp = document.createElement('div');
    cnHelp.className = 'mcp-form-help';
    cnHelp.id = 'llm-modal-custom-name-help';
    cnHelp.textContent = 'Shown on the card. Safe to rename later.';
    cnGroup.appendChild(cnHelp);
    customFields.appendChild(cnGroup);

    // Wire protocol
    const cwGroup = document.createElement('div');
    cwGroup.className = 'mcp-form-group';
    const cwLabel = document.createElement('label');
    cwLabel.className = 'mcp-form-label';
    cwLabel.textContent = 'API Type';
    cwLabel.setAttribute('for', 'llm-modal-custom-wire');
    cwGroup.appendChild(cwLabel);
    const cwSelect = document.createElement('select');
    cwSelect.className = 'mcp-form-input';
    cwSelect.id = 'llm-modal-custom-wire';
    for (const [value, label] of [
      ['openai', 'OpenAI-compatible'],
      ['anthropic', 'Anthropic'],
    ]) {
      const o = document.createElement('option');
      o.value = value;
      o.textContent = label;
      cwSelect.appendChild(o);
    }
    cwSelect.addEventListener('change', (e) => this._syncCustomWireHints(e.target.value));
    cwGroup.appendChild(cwSelect);
    customFields.appendChild(cwGroup);

    // Context window
    const ccGroup = document.createElement('div');
    ccGroup.className = 'mcp-form-group';
    const ccLabel = document.createElement('label');
    ccLabel.className = 'mcp-form-label';
    ccLabel.textContent = 'Context Window';
    ccLabel.setAttribute('for', 'llm-modal-custom-context');
    ccGroup.appendChild(ccLabel);
    const ccInput = document.createElement('input');
    ccInput.className = 'mcp-form-input';
    ccInput.type = 'number';
    ccInput.id = 'llm-modal-custom-context';
    ccGroup.appendChild(ccInput);
    const ccHelp = document.createElement('div');
    ccHelp.className = 'mcp-form-help';
    ccHelp.id = 'llm-modal-custom-context-help';
    ccGroup.appendChild(ccHelp);
    customFields.appendChild(ccGroup);

    // Streaming
    const csGroup = document.createElement('div');
    csGroup.className = 'mcp-form-group llm-active-group';
    const csLabel = document.createElement('label');
    csLabel.className = 'llm-active-label';
    const csCheckbox = document.createElement('input');
    csCheckbox.type = 'checkbox';
    csCheckbox.id = 'llm-modal-custom-streaming';
    const csText = document.createElement('span');
    csText.textContent = 'Enable streaming (SSE)';
    csLabel.appendChild(csCheckbox);
    csLabel.appendChild(csText);
    csGroup.appendChild(csLabel);
    customFields.appendChild(csGroup);

    // Accent colour
    const caGroup = document.createElement('div');
    caGroup.className = 'mcp-form-group';
    const caLabel = document.createElement('label');
    caLabel.className = 'mcp-form-label';
    caLabel.textContent = 'Card Colour';
    caGroup.appendChild(caLabel);
    const caSwatches = document.createElement('div');
    caSwatches.className = 'llm-accent-swatches';
    caSwatches.id = 'llm-modal-custom-accent';
    for (const colour of ['', '#7c5cff', '#0b8043', '#d93025', '#f9ab00', '#1a73e8', '#8430ce']) {
      const swatch = document.createElement('button');
      swatch.type = 'button';
      swatch.className = 'llm-accent-swatch';
      swatch.dataset.accent = colour;
      swatch.setAttribute('aria-label', colour ? `Accent ${colour}` : 'Default accent');
      if (colour) {
        swatch.style.background = colour;
      } else {
        swatch.classList.add('llm-accent-default');
      }
      swatch.addEventListener('click', () => {
        caSwatches
          .querySelectorAll('.llm-accent-swatch')
          .forEach((el) => el.classList.remove('selected'));
        swatch.classList.add('selected');
      });
      caSwatches.appendChild(swatch);
    }
    caGroup.appendChild(caSwatches);
    customFields.appendChild(caGroup);

    form.appendChild(customFields);


    // Set as active checkbox
    const activeGroup = document.createElement('div');
    activeGroup.className = 'mcp-form-group llm-active-group';
    const activeLabel = document.createElement('label');
    activeLabel.className = 'llm-active-label';
    const activeCheckbox = document.createElement('input');
    activeCheckbox.type = 'checkbox';
    activeCheckbox.id = 'llm-modal-set-active';
    const activeText = document.createElement('span');
    activeText.textContent = 'Set as active provider';
    activeLabel.appendChild(activeCheckbox);
    activeLabel.appendChild(activeText);
    activeGroup.appendChild(activeLabel);
    form.appendChild(activeGroup);

    // Status message
    const statusMsg = document.createElement('div');
    statusMsg.className = 'llm-modal-status';
    statusMsg.id = 'llm-modal-status';
    form.appendChild(statusMsg);

    content.appendChild(form);

    // Actions
    const actions = document.createElement('div');
    actions.className = 'mcp-modal-actions';
    const cancelBtn = document.createElement('button');
    cancelBtn.className = 'mcp-btn-secondary';
    cancelBtn.type = 'button';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.addEventListener('click', () => this._closeLlmModal());
    const saveBtn = document.createElement('button');
    saveBtn.className = 'mcp-btn-primary';
    saveBtn.type = 'button';
    saveBtn.id = 'llm-modal-save';
    saveBtn.textContent = 'Save';
    saveBtn.addEventListener('click', () => {
      // `_llmEditCustomId` is the mode flag: undefined = builtin provider,
      // null = creating a custom one, string = editing that custom one.
      if (this._llmEditCustomId !== undefined) {
        this._saveCustomProvider();
      } else {
        this._saveLlmProvider();
      }
    });
    actions.appendChild(cancelBtn);
    actions.appendChild(saveBtn);
    content.appendChild(actions);

    modal.appendChild(content);
    document.body.appendChild(modal);
    this._llmModal = modal;
  },

  async _showProviderModal(provider) {
    this._ensureLlmModal();
    this._llmEditProvider = provider.id;
    this._llmEditCustomId = undefined;
    const customFieldsEl = document.getElementById('llm-modal-custom-fields');
    if (customFieldsEl) customFieldsEl.style.display = 'none';

    const title = document.getElementById('llm-modal-title');
    title.textContent = `Configure ${provider.display_name || provider.id}`;

    const subtitle = document.getElementById('llm-modal-subtitle');
    const typeLabels = { cli: 'CLI provider', local: 'Local provider', agent: 'Agent provider', service: 'Cloud API provider' };
    subtitle.textContent = typeLabels[provider.type] || 'Cloud API provider';

    const isOpenClaw = provider.id === 'openclaw';

    // Show/hide ToS warning for providers requiring one (e.g. antigravity)
    const tosWarn = document.getElementById('llm-modal-tos-warning');
    if (tosWarn) tosWarn.style.display = provider.id === 'antigravity' ? '' : 'none';

    // Reset fields
    document.getElementById('llm-modal-apikey').value = '';
    document.getElementById('llm-modal-apikey').placeholder = 'Enter API key...';
    document.getElementById('llm-modal-model').value = '';
    document.getElementById('llm-modal-baseurl').value = '';
    document.getElementById('llm-modal-set-active').checked = false;
    document.getElementById('llm-modal-status').textContent = '';
    document.getElementById('llm-modal-status').className = 'llm-modal-status';

    // Hide base URL for CLI providers (they don't support it)
    const baseUrlGroup = document.getElementById('llm-modal-baseurl-group');
    baseUrlGroup.style.display = provider.type === 'cli' ? 'none' : '';

    // Show/hide OpenClaw-specific fields
    const ocFields = document.getElementById('llm-modal-openclaw-fields');
    ocFields.style.display = isOpenClaw ? '' : 'none';
    if (isOpenClaw) {
      // Reset OpenClaw fields
      document.getElementById('llm-modal-oc-provider-name').value = '';
      document.getElementById('llm-modal-oc-api-type').value = 'openai-completions';
      document.getElementById('llm-modal-oc-context-window').value = '';
      document.getElementById('llm-modal-oc-max-tokens').value = '';
      document.getElementById('llm-modal-oc-reasoning').checked = false;

      // Change labels for OpenClaw context
      document.getElementById('llm-modal-model').placeholder = 'e.g. gpt-4o';
      document.getElementById('llm-modal-baseurl').placeholder = 'e.g. https://api.openai.com/v1';
    } else {
      document.getElementById('llm-modal-model').placeholder = 'Leave empty for default';
      document.getElementById('llm-modal-baseurl').placeholder = 'Leave empty for default endpoint';
    }

    // Load current config from agent
    try {
      const data = await this._sendAgentCommand('config.llm.get', { provider: provider.id });
      if (data) {
        if (data.has_api_key) {
          document.getElementById('llm-modal-apikey').placeholder =
            `Current: ${data.api_key || 'configured'}`;
        }
        if (data.model) {
          document.getElementById('llm-modal-model').value = data.model;
        }
        if (data.base_url) {
          document.getElementById('llm-modal-baseurl').value = data.base_url;
        }
        document.getElementById('llm-modal-set-active').checked = !!data.active;
        document.getElementById('llm-modal-model-help').textContent =
          `Default: ${data.default_model || 'unknown'}`;
        document.getElementById('llm-modal-key-help').textContent = data.has_api_key
          ? 'Key is configured. Leave blank to keep current.'
          : 'Required for this provider.';
      }
    } catch (e) {
      console.warn('Failed to load provider config:', e);
    }

    // Load OpenClaw model config if available
    if (isOpenClaw) {
      try {
        const ocData = await this._sendAgentCommand('config.openclaw.model.list');
        if (ocData?.success && ocData.providers) {
          // Pick the first configured provider or use primary_model to find it
          const primaryModel = ocData.primary_model || '';
          const primaryProvider = primaryModel.split('/')[0] || '';
          const providerNames = Object.keys(ocData.providers);
          const provName = primaryProvider || providerNames[0] || '';

          if (provName && ocData.providers[provName]) {
            const prov = ocData.providers[provName];
            document.getElementById('llm-modal-oc-provider-name').value = provName;
            if (prov.baseUrl) document.getElementById('llm-modal-baseurl').value = prov.baseUrl;
            if (prov.api) document.getElementById('llm-modal-oc-api-type').value = prov.api;
            // Load first model's details
            if (prov.models && prov.models.length > 0) {
              const m = prov.models[0];
              if (m.id) document.getElementById('llm-modal-model').value = m.id;
              if (m.contextWindow) document.getElementById('llm-modal-oc-context-window').value = m.contextWindow;
              if (m.maxTokens) document.getElementById('llm-modal-oc-max-tokens').value = m.maxTokens;
              document.getElementById('llm-modal-oc-reasoning').checked = !!m.reasoning;
            }
          }
        }
      } catch (e) {
        console.warn('Failed to load OpenClaw config:', e);
      }
    }

    this._llmModal.classList.add('show');
    setTimeout(() => {
      document.getElementById(isOpenClaw ? 'llm-modal-oc-provider-name' : 'llm-modal-apikey').focus();
    }, 50);
  },

  _closeLlmModal() {
    if (this._llmModal) {
      this._llmModal.classList.remove('show');
    }
  },

  async _saveLlmProvider() {
    const providerId = this._llmEditProvider;
    if (!providerId) return;

    const statusEl = document.getElementById('llm-modal-status');
    const saveBtn = document.getElementById('llm-modal-save');

    const apiKey = document.getElementById('llm-modal-apikey').value.trim();
    const model = document.getElementById('llm-modal-model').value.trim();
    const baseUrl = document.getElementById('llm-modal-baseurl').value.trim();
    const setActive = document.getElementById('llm-modal-set-active').checked;

    // Leaving on-device while the latch is on cannot take effect until a
    // restart. Asked here, before any RPC, so a refusal leaves nothing
    // half-saved.
    if (setActive && (await this._localIsLatched())) {
      const logic = await this._ensureLocalLogic();
      if (logic.needsExitConfirm(true, providerId)) {
        const go = await this._confirmExitLocal(providerId);
        if (!go) {
          statusEl.textContent = 'Kept on-device.';
          statusEl.className = 'llm-modal-status';
          return;
        }
      }
    }

    saveBtn.disabled = true;
    statusEl.textContent = 'Saving...';
    statusEl.className = 'llm-modal-status';

    try {
      if (providerId === 'openclaw') {
        // OpenClaw: dual save — config.toml + openclaw.json
        const providerName = document.getElementById('llm-modal-oc-provider-name').value.trim();
        const apiType = document.getElementById('llm-modal-oc-api-type').value;
        const contextWindow = parseInt(document.getElementById('llm-modal-oc-context-window').value) || 92160;
        const maxTokens = parseInt(document.getElementById('llm-modal-oc-max-tokens').value) || 32768;
        const reasoning = document.getElementById('llm-modal-oc-reasoning').checked;

        if (!providerName) {
          statusEl.textContent = 'Provider Name is required';
          statusEl.className = 'llm-modal-status error';
          saveBtn.disabled = false;
          return;
        }

        // 1) Try to save OpenClaw model config (openclaw.json)
        //    This may fail if OpenClaw CLI is not installed — continue anyway
        let openclawSaved = false;
        try {
          await this._sendAgentCommand('config.openclaw.model.set', {
            provider_name: providerName,
            base_url: baseUrl,
            api_key: apiKey,
            api_type: apiType,
            model_id: model || 'gpt-4o',
            model_name: model || 'gpt-4o',
            context_window: contextWindow,
            max_tokens: maxTokens,
            reasoning: reasoning,
            set_as_primary: setActive,
          });
          openclawSaved = true;
        } catch (ocErr) {
          console.warn('OpenClaw config save failed (CLI may not be installed):', ocErr.message);
        }

        // 2) Save NevoFlux config.toml (provider = openclaw) — always runs
        const llmParams = {
          provider: 'openclaw',
          set_active: setActive,
          model: providerName + '/' + (model || 'gpt-4o'),
        };
        if (apiKey) llmParams.api_key = apiKey;
        llmParams.base_url = baseUrl;
        await this._sendAgentCommand('config.llm.set', llmParams);

        if (!openclawSaved) {
          statusEl.textContent = 'NevoFlux config saved. OpenClaw CLI not available — install it to sync model config.';
          statusEl.className = 'llm-modal-status success';
          await this._populateLlmProviders();
          saveBtn.disabled = false;
          return;
        }
      } else {
        // Standard provider: single save
        const params = { provider: providerId, set_active: setActive };
        if (apiKey) params.api_key = apiKey;
        if (model !== undefined) params.model = model;
        params.base_url = baseUrl;
        await this._sendAgentCommand('config.llm.set', params);
      }

      statusEl.textContent = 'Saved successfully!';
      statusEl.className = 'llm-modal-status success';

      // Refresh the provider grid
      await this._populateLlmProviders();

      // Close after a brief delay
      setTimeout(() => this._closeLlmModal(), 600);
    } catch (e) {
      statusEl.textContent = `Error: ${e.message}`;
      statusEl.className = 'llm-modal-status error';
    } finally {
      saveBtn.disabled = false;
    }
  },

  // ── MCP Servers Section ────────────────────────────────

  // ── Space Souls ─────────────────────────────────────────
  //
  // Users think in Spaces; the daemon only ever sees a container. The table is
  // therefore keyed by container, with the Spaces that use it named on the row —
  // including when several share one, which is legal and means they share the
  // assistant and its memory too.

  _renderSoulsSection() {
    const section = this._createSection('space-souls', 'Space Souls');
    const group = this._createGroup('Space Souls');

    const desc = document.createElement('p');
    desc.className = 'section-desc';
    desc.textContent =
      "Give each Space its own assistant. A soul is bound to the Space's container — " +
      'it brings its own personality, tools and private memory, and never crosses ' +
      'into another container.';
    group.appendChild(desc);

    const list = document.createElement('ul');
    list.className = 'binding-list';
    list.id = 'soul-binding-list';
    group.appendChild(list);

    section.appendChild(group);

    const libGroup = this._createGroup('Souls library');

    const newBtn = document.createElement('button');
    newBtn.className = 'new-soul-btn';
    newBtn.textContent = '＋ New soul';
    newBtn.addEventListener('click', () => this._openSoulEditor(null));
    libGroup.appendChild(newBtn);

    const libList = document.createElement('ul');
    libList.className = 'soul-lib-list';
    libList.id = 'soul-lib-list';
    libGroup.appendChild(libList);

    const hint = document.createElement('div');
    hint.className = 'folder-hint';
    hint.innerHTML =
      'Souls are folders in <code>~/.config/nevoflux/agents/</code> — each needs an ' +
      '<code>IDENTITY.md</code> (name and avatar) and a <code>SOUL.md</code> (personality). ' +
      'Edits reload automatically.';
    libGroup.appendChild(hint);

    section.appendChild(libGroup);
    section.appendChild(this._renderSoulEditor());
    return section;
  },

  // Load souls, bindings and Spaces, then draw the table.
  async _loadSouls() {
    try {
      const [soulsData, bindingsData] = await Promise.all([
        this._sendAgentCommand('soul.list'),
        this._sendAgentCommand('soul.bindings'),
      ]);
      this._souls = soulsData?.souls || [];
      this._soulBindings = bindingsData?.bindings || {};
    } catch (e) {
      console.error('[NevoFlux] Could not load souls:', e);
      this._souls = [];
      this._soulBindings = {};
    }

    // Spaces come from chrome, not the daemon: it never learns their names.
    // This is a chrome page with no `browser.*` globals, so it asks the parent
    // actor (which can reach gZenWorkspaces) rather than a WebExtension API.
    try {
      this._spaces = (await NevofluxPage.sendQuery('spaces:list')) || [];
    } catch (e) {
      console.warn('[NevoFlux] Could not list Spaces; showing containers only:', e);
      this._spaces = [];
    }

    this._renderSoulBindings();
    this._renderSoulLibrary();
  },

  // One row per container, because that is what a binding keys on.
  //
  // Rows come from three places: the Spaces that exist, the container-less
  // default (always last), and any binding left pointing at a container no Space
  // uses any more — which would otherwise be invisible and unremovable.
  _soulBindingRows() {
    const byContainer = new Map();

    for (const space of this._spaces || []) {
      const id = space.cookieStoreId;
      if (!byContainer.has(id)) {
        byContainer.set(id, { cookieStoreId: id, spaces: [], stale: false });
      }
      byContainer.get(id).spaces.push(space.name || 'Untitled');
    }

    for (const container of Object.keys(this._soulBindings || {})) {
      if (!byContainer.has(container)) {
        byContainer.set(container, { cookieStoreId: container, spaces: [], stale: true });
      }
    }

    if (!byContainer.has('firefox-default')) {
      byContainer.set('firefox-default', {
        cookieStoreId: 'firefox-default',
        spaces: [],
        stale: false,
      });
    }

    const rows = [...byContainer.values()];
    // Container-less last: it is the fallback, not a Space.
    rows.sort((a, b) => {
      if (a.cookieStoreId === 'firefox-default') return 1;
      if (b.cookieStoreId === 'firefox-default') return -1;
      return (a.spaces[0] || '').localeCompare(b.spaces[0] || '');
    });
    return rows;
  },

  _renderSoulBindings() {
    const list = document.getElementById('soul-binding-list');
    if (!list) return;
    list.innerHTML = '';

    for (const row of this._soulBindingRows()) {
      list.appendChild(this._renderSoulBindingRow(row));
    }
  },

  _renderSoulBindingRow(row) {
    const li = document.createElement('li');
    li.className = 'binding-row';

    const slug = this._soulBindings?.[row.cookieStoreId];
    const soul = (this._souls || []).find((s) => s.slug === slug);
    const isDefault = row.cookieStoreId === 'firefox-default';

    if (!soul) li.classList.add('unbound');
    if (row.stale) li.classList.add('stale');
    if (isDefault) li.classList.add('default-row');

    // The face: this is what the floating avatar will look like here.
    const glyph = document.createElement('div');
    glyph.className = 'binding-glyph';
    if (soul?.avatar) {
      const img = document.createElement('img');
      img.src = soul.avatar;
      img.alt = '';
      glyph.appendChild(img);
    } else {
      const empty = document.createElement('span');
      empty.className = 'glyph-empty';
      glyph.appendChild(empty);
    }
    li.appendChild(glyph);

    const info = document.createElement('div');
    info.className = 'binding-info';

    const spaces = document.createElement('div');
    spaces.className = 'binding-spaces';
    if (row.stale) {
      spaces.textContent = 'No Space uses this container';
    } else if (isDefault) {
      spaces.textContent = 'No container';
    } else {
      spaces.textContent = row.spaces.join(' · ');
      if (row.spaces.length > 1) {
        const badge = document.createElement('span');
        badge.className = 'badge-shared';
        badge.textContent = 'shared container';
        badge.title =
          'These Spaces share one container, so they share the same soul and memory.';
        spaces.appendChild(document.createTextNode(' '));
        spaces.appendChild(badge);
      }
    }
    info.appendChild(spaces);

    if (isDefault) {
      const note = document.createElement('div');
      note.className = 'row-note';
      note.textContent =
        'Tabs outside any container always use the default assistant. To give a Space ' +
        'its own soul, assign it a container first (Space settings → Set Profile).';
      info.appendChild(note);
    } else {
      const container = document.createElement('div');
      container.className = 'binding-container';
      const code = document.createElement('code');
      code.textContent = row.cookieStoreId;
      container.appendChild(code);
      if (row.stale && slug) {
        container.appendChild(document.createTextNode(` — still bound to ${slug}`));
      }
      info.appendChild(container);
    }
    li.appendChild(info);

    const actions = document.createElement('div');
    actions.className = 'binding-actions';

    if (row.stale) {
      const remove = document.createElement('button');
      remove.className = 'stale-remove';
      remove.textContent = 'Remove binding';
      remove.addEventListener('click', () => this._unbindSoul(row.cookieStoreId));
      actions.appendChild(remove);
    } else {
      const picker = document.createElement('select');
      picker.className = 'soul-picker';
      picker.disabled = isDefault;

      const none = document.createElement('option');
      none.value = '';
      none.textContent = 'Default assistant';
      picker.appendChild(none);

      for (const s of this._souls || []) {
        const opt = document.createElement('option');
        opt.value = s.slug;
        opt.textContent = s.name;
        if (s.slug === slug) opt.selected = true;
        picker.appendChild(opt);
      }

      picker.addEventListener('change', () => {
        if (picker.value) {
          this._bindSoul(row.cookieStoreId, picker.value);
        } else {
          this._unbindSoul(row.cookieStoreId);
        }
      });
      actions.appendChild(picker);
    }

    li.appendChild(actions);
    return li;
  },

  _renderSoulLibrary() {
    const list = document.getElementById('soul-lib-list');
    if (!list) return;
    list.innerHTML = '';

    if (!(this._souls || []).length) {
      const empty = document.createElement('li');
      empty.className = 'soul-lib-empty';
      empty.textContent =
        'No souls yet. Create a folder under agents/ with an IDENTITY.md and a SOUL.md.';
      list.appendChild(empty);
      return;
    }

    // Reverse map so each soul says where it is used, not just that it exists.
    const usedBy = new Map();
    for (const [container, slug] of Object.entries(this._soulBindings || {})) {
      const space = (this._spaces || []).find((sp) => sp.cookieStoreId === container);
      if (space) usedBy.set(slug, space.name || 'Untitled');
    }

    for (const soul of this._souls) {
      const li = document.createElement('li');
      li.className = 'soul-lib-row';
      li.setAttribute('role', 'button');
      li.tabIndex = 0;
      li.addEventListener('click', () => this._openSoulEditor(soul.slug));
      li.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          this._openSoulEditor(soul.slug);
        }
      });

      if (soul.avatar) {
        const img = document.createElement('img');
        img.src = soul.avatar;
        img.alt = '';
        li.appendChild(img);
      }

      const body = document.createElement('div');
      body.className = 'soul-lib-body';

      const name = document.createElement('div');
      name.className = 'soul-lib-name';
      name.textContent = soul.name;
      const code = document.createElement('code');
      code.textContent = `agents/${soul.slug}/`;
      name.appendChild(code);
      body.appendChild(name);

      if (soul.description) {
        const d = document.createElement('div');
        d.className = 'soul-lib-desc';
        d.textContent = soul.description;
        body.appendChild(d);
      }
      li.appendChild(body);

      const usage = document.createElement('div');
      usage.className = 'soul-lib-usage';
      usage.textContent = usedBy.has(soul.slug) ? `Bound to ${usedBy.get(soul.slug)}` : 'Not bound';
      li.appendChild(usage);

      list.appendChild(li);
    }
  },

  async _bindSoul(container, slug) {
    try {
      const data = await this._sendAgentCommand('soul.bind', { container, slug });
      this._soulBindings = data?.bindings || this._soulBindings;
      this._renderSoulBindings();
      this._renderSoulLibrary();
      this._showSaveIndicator();
    } catch (e) {
      console.error('[NevoFlux] Could not bind soul:', e);
      // Redraw from what the daemon last told us, so the picker never shows a
      // binding that did not take.
      this._renderSoulBindings();
    }
  },

  async _unbindSoul(container) {
    try {
      const data = await this._sendAgentCommand('soul.unbind', { container });
      this._soulBindings = data?.bindings || this._soulBindings;
      this._renderSoulBindings();
      this._renderSoulLibrary();
      this._showSaveIndicator();
    } catch (e) {
      console.error('[NevoFlux] Could not remove binding:', e);
      this._renderSoulBindings();
    }
  },

  // ── Soul editor ─────────────────────────────────────────
  //
  // Machine config (name, avatar, the whitelists) is edited as form fields and
  // serialized by the daemon: a frontmatter typo makes a soul vanish from the
  // list, so nobody should have to hand-write YAML to rename their assistant.
  // The four bodies stay plain text, because that is what they are.

  // Which body goes in which file, and what to say about it. `optional` files
  // fall back to the user's global section when left empty — that is usually what
  // you want, so the placeholder says so.
  _soulFields: [
    {
      key: 'soul',
      file: 'SOUL.md',
      title: 'Personality',
      hint: "How this assistant thinks and speaks. Replaces the global personality while this soul is active.",
      rows: 9,
      optional: false,
    },
    {
      key: 'identity',
      file: 'IDENTITY.md',
      title: 'Identity notes',
      hint: 'A short self-description, injected as the identity section.',
      rows: 3,
      optional: false,
    },
    {
      key: 'tools',
      file: 'TOOLS.md',
      title: 'Tool guidance',
      hint: 'Leave empty to keep the global tool guidance.',
      rows: 4,
      optional: true,
    },
    {
      key: 'agents',
      file: 'AGENTS.md',
      title: 'Subagent guidance',
      hint: 'Leave empty to keep the global subagent guidance.',
      rows: 4,
      optional: true,
    },
  ],

  _renderSoulEditor() {
    const wrap = document.createElement('div');
    wrap.className = 'soul-editor';
    wrap.id = 'soul-editor';
    wrap.hidden = true;

    const back = document.createElement('button');
    back.className = 'editor-back';
    back.textContent = '← Space Souls';
    back.addEventListener('click', () => this._closeSoulEditor());
    wrap.appendChild(back);

    // Head: avatar, name, slug, description.
    const head = document.createElement('div');
    head.className = 'editor-head';

    const avatarBtn = document.createElement('button');
    avatarBtn.className = 'avatar-edit';
    avatarBtn.id = 'soul-avatar-btn';
    avatarBtn.title = 'Change avatar — saved as avatar.png in the soul folder';
    const avatarImg = document.createElement('img');
    avatarImg.id = 'soul-avatar-img';
    avatarImg.alt = '';
    avatarBtn.appendChild(avatarImg);
    const badge = document.createElement('span');
    badge.className = 'avatar-badge';
    badge.setAttribute('aria-hidden', 'true');
    badge.textContent = '✎';
    avatarBtn.appendChild(badge);
    head.appendChild(avatarBtn);

    const file = document.createElement('input');
    file.type = 'file';
    file.accept = 'image/png,image/jpeg';
    file.hidden = true;
    file.id = 'soul-avatar-file';
    head.appendChild(file);
    avatarBtn.addEventListener('click', () => file.click());
    file.addEventListener('change', () => this._pickSoulAvatar(file));

    const fields = document.createElement('div');
    fields.className = 'editor-title-fields';

    const nameRow = document.createElement('div');
    nameRow.className = 'editor-name-row';
    const name = document.createElement('input');
    name.className = 'ed-name';
    name.id = 'soul-name';
    name.setAttribute('aria-label', 'Soul name');
    nameRow.appendChild(name);
    const slug = document.createElement('span');
    slug.className = 'ed-slug';
    slug.id = 'soul-slug';
    nameRow.appendChild(slug);
    fields.appendChild(nameRow);

    const desc = document.createElement('input');
    desc.className = 'ed-desc';
    desc.id = 'soul-description';
    desc.setAttribute('aria-label', 'Description');
    desc.placeholder = 'One line about what this assistant is for';
    fields.appendChild(desc);
    head.appendChild(fields);

    const state = document.createElement('span');
    state.className = 'save-state';
    state.id = 'soul-save-state';
    head.appendChild(state);

    wrap.appendChild(head);

    // A name that clashes or cannot be mentioned is refused by the daemon; say so
    // here rather than letting Save look like it worked.
    const error = document.createElement('div');
    error.className = 'editor-error';
    error.id = 'soul-editor-error';
    error.hidden = true;
    wrap.appendChild(error);

    for (const field of this._soulFields) {
      wrap.appendChild(this._renderSoulField(field));
    }

    wrap.appendChild(this._renderSoulAdvanced());

    const actions = document.createElement('div');
    actions.className = 'editor-actions';
    const save = document.createElement('button');
    save.className = 'editor-save';
    save.textContent = 'Save';
    save.addEventListener('click', () => this._saveSoul());
    actions.appendChild(save);
    wrap.appendChild(actions);

    const danger = document.createElement('div');
    danger.className = 'danger-zone';
    danger.id = 'soul-danger';
    const dangerText = document.createElement('span');
    dangerText.id = 'soul-danger-text';
    danger.appendChild(dangerText);
    const del = document.createElement('button');
    del.textContent = 'Delete soul';
    del.addEventListener('click', () => this._deleteSoul());
    danger.appendChild(del);
    wrap.appendChild(danger);

    return wrap;
  },

  _renderSoulField(field) {
    const group = document.createElement('div');
    group.className = 'settings-group editor-group';

    const head = document.createElement('div');
    head.className = 'group-head';

    const h2 = document.createElement('h2');
    h2.textContent = field.title;
    const code = document.createElement('code');
    code.textContent = field.file;
    h2.appendChild(code);
    if (field.optional) {
      const tag = document.createElement('span');
      tag.className = 'opt-tag';
      tag.textContent = 'optional';
      h2.appendChild(tag);
    }
    head.appendChild(h2);

    const draftWrap = document.createElement('span');
    draftWrap.id = `soul-draft-wrap-${field.key}`;
    const draft = document.createElement('button');
    draft.className = 'draft-btn';
    draft.textContent = '✨ Draft';
    draft.addEventListener('click', () => this._draftSoulField(field.key));
    draftWrap.appendChild(draft);
    head.appendChild(draftWrap);

    group.appendChild(head);

    const hint = document.createElement('p');
    hint.className = 'field-hint';
    hint.textContent = field.hint;
    group.appendChild(hint);

    const area = document.createElement('textarea');
    area.className = 'ed-textarea';
    area.id = `soul-field-${field.key}`;
    area.rows = field.rows;
    if (field.optional) {
      area.placeholder = `Empty — the global ${field.file.replace('.md', '')} section applies.`;
    }
    group.appendChild(area);

    return group;
  },

  _renderSoulAdvanced() {
    const details = document.createElement('details');
    details.className = 'adv-details';

    const summary = document.createElement('summary');
    summary.textContent = 'Advanced — capabilities';
    details.appendChild(summary);

    const grid = document.createElement('div');
    grid.className = 'adv-grid';

    for (const adv of [
      {
        key: 'allowed_tools',
        label: 'Allowed tools',
        placeholder: 'web_search\nbrain_*',
        hint: 'One per line, patterns allowed. Tools not listed are unavailable to this soul. Empty = all tools of the current mode.',
      },
    ]) {
      const field = document.createElement('div');
      field.className = 'adv-field';

      const label = document.createElement('label');
      label.setAttribute('for', `soul-adv-${adv.key}`);
      label.textContent = adv.label;
      field.appendChild(label);

      const area = document.createElement('textarea');
      area.className = 'ed-textarea';
      area.id = `soul-adv-${adv.key}`;
      area.placeholder = adv.placeholder;
      field.appendChild(area);

      const hint = document.createElement('p');
      hint.className = 'field-hint';
      hint.textContent = adv.hint;
      field.appendChild(hint);

      grid.appendChild(field);
    }

    details.appendChild(grid);
    return details;
  },

  /// Open a soul for editing, or start a new one when `slug` is null.
  async _openSoulEditor(slug) {
    const editor = document.getElementById('soul-editor');
    if (!editor) return;

    this._editingSlug = slug;
    this._soulEditorDraft = null;
    this._showSoulEditorError(null);

    let soul = {
      name: '',
      description: '',
      avatar: null,
      identity: '',
      soul: '',
      tools: '',
      agents: '',
      allowed_tools: [],
    };

    if (slug) {
      try {
        soul = { ...soul, ...(await this._sendAgentCommand('soul.read', { slug })) };
      } catch (e) {
        console.error('[NevoFlux] Could not read soul:', e);
        this._showSoulEditorError('That soul could not be opened.');
      }
    }

    document.getElementById('soul-name').value = soul.name || '';
    document.getElementById('soul-description').value = soul.description || '';
    document.getElementById('soul-slug').textContent = slug ? `agents/${slug}/` : 'agents/…/';

    const img = document.getElementById('soul-avatar-img');
    img.src = soul.avatar || '';
    img.hidden = !soul.avatar;

    for (const field of this._soulFields) {
      document.getElementById(`soul-field-${field.key}`).value = soul[field.key] || '';
    }
    document.getElementById('soul-adv-allowed_tools').value = (soul.allowed_tools || []).join('\n');

    // A soul that does not exist yet has nothing to delete, and no avatar to set:
    // both need a folder first.
    document.getElementById('soul-danger').hidden = !slug;
    document.getElementById('soul-avatar-btn').disabled = !slug;
    if (slug) {
      document.getElementById('soul-danger-text').textContent =
        `Deletes agents/${slug}/ and removes its bindings. Chats it wrote keep their labels.`;
    }
    document.getElementById('soul-save-state').textContent = slug ? 'Saved' : '';

    for (const section of document.querySelectorAll('#section-space-souls > .settings-group')) {
      section.hidden = true;
    }
    editor.hidden = false;
    document.getElementById(slug ? 'soul-name' : 'soul-name').focus();
  },

  _closeSoulEditor() {
    const editor = document.getElementById('soul-editor');
    if (editor) editor.hidden = true;
    for (const section of document.querySelectorAll('#section-space-souls > .settings-group')) {
      section.hidden = false;
    }
    this._editingSlug = null;
    this._loadSouls();
  },

  _showSoulEditorError(message) {
    const error = document.getElementById('soul-editor-error');
    if (!error) return;
    error.textContent = message || '';
    error.hidden = !message;
  },

  _soulEditorFields() {
    const fields = {
      name: document.getElementById('soul-name').value,
      description: document.getElementById('soul-description').value,
      allowed_tools: document.getElementById('soul-adv-allowed_tools').value,
    };
    for (const field of this._soulFields) {
      fields[field.key] = document.getElementById(`soul-field-${field.key}`).value;
    }
    return fields;
  },

  async _saveSoul() {
    const fields = this._soulEditorFields();
    this._showSoulEditorError(null);

    // A new soul needs a folder before it can be written to.
    let slug = this._editingSlug;
    if (!slug) {
      try {
        const created = await this._sendAgentCommand('soul.create', { name: fields.name });
        slug = created.slug;
        this._editingSlug = slug;
        document.getElementById('soul-slug').textContent = `agents/${slug}/`;
        document.getElementById('soul-danger').hidden = false;
        document.getElementById('soul-avatar-btn').disabled = false;
        document.getElementById('soul-danger-text').textContent =
          `Deletes agents/${slug}/ and removes its bindings. Chats it wrote keep their labels.`;
      } catch (e) {
        this._showSoulEditorError(e?.message || 'That soul could not be created.');
        return;
      }
    }

    try {
      await this._sendAgentCommand('soul.write', { slug, ...fields });
      document.getElementById('soul-save-state').textContent = 'Saved';
      this._showSaveIndicator();
    } catch (e) {
      // The daemon refuses a clashing or unmentionable name; say which, rather
      // than leaving Save looking like it worked.
      this._showSoulEditorError(e?.message || 'That soul could not be saved.');
    }
  },

  async _deleteSoul() {
    if (!this._editingSlug) return;
    try {
      await this._sendAgentCommand('soul.delete', { slug: this._editingSlug });
      this._closeSoulEditor();
    } catch (e) {
      this._showSoulEditorError(e?.message || 'That soul could not be deleted.');
    }
  },

  async _pickSoulAvatar(input) {
    const file = input.files?.[0];
    if (!file || !this._editingSlug) return;

    try {
      const dataUri = await this._readSoulAvatar(file);
      const result = await this._sendAgentCommand('soul.set_avatar', {
        slug: this._editingSlug,
        data_uri: dataUri,
      });
      const img = document.getElementById('soul-avatar-img');
      img.src = result?.avatar || dataUri;
      img.hidden = false;
      this._showSaveIndicator();
    } catch (e) {
      this._showSoulEditorError(e?.message || 'That image could not be used.');
    } finally {
      input.value = '';
    }
  },

  /// Scale an image down before sending it, so a soul's folder never holds a 4MB
  /// avatar for a 40px chip.
  _readSoulAvatar(file) {
    const MAX = 256;
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onerror = () => reject(new Error('That image could not be read.'));
      reader.onload = () => {
        const img = new Image();
        img.onerror = () => reject(new Error('That image could not be read.'));
        img.onload = () => {
          const scale = Math.min(1, MAX / Math.max(img.width, img.height));
          const canvas = document.createElement('canvas');
          canvas.width = Math.round(img.width * scale);
          canvas.height = Math.round(img.height * scale);
          canvas.getContext('2d').drawImage(img, 0, 0, canvas.width, canvas.height);
          resolve(canvas.toDataURL('image/png'));
        };
        img.src = reader.result;
      };
      reader.readAsDataURL(file);
    });
  },

  /// Ask the model for a first draft of one field.
  ///
  /// The draft lands in the textarea, not on disk: saving is still the user's
  /// move, and Undo puts back what was there.
  async _draftSoulField(key) {
    const wrap = document.getElementById(`soul-draft-wrap-${key}`);
    const button = wrap?.querySelector('.draft-btn');
    const area = document.getElementById(`soul-field-${key}`);
    if (!button || !area) return;

    button.disabled = true;
    button.textContent = 'Drafting…';
    this._showSoulEditorError(null);

    try {
      // Drafting streams a full reply from the model, which on a slow endpoint
      // can take well over a minute. Keep this at or above the background
      // stale-command deadline so the bridge does not give up first.
      const result = await this._sendAgentCommand(
        'soul.generate',
        {
          target: key,
          slug: this._editingSlug || undefined,
          ...this._soulEditorFields(),
        },
        150000
      );

      const previous = area.value;
      area.value = result.content;

      wrap.querySelector('.undo-chip')?.remove();
      const undo = document.createElement('button');
      undo.className = 'undo-chip';
      undo.textContent = '↩ Undo';
      undo.addEventListener('click', () => {
        area.value = previous;
        undo.remove();
      });
      wrap.appendChild(undo);
      // Once they start editing, the draft is theirs and Undo is noise.
      area.addEventListener('input', () => undo.remove(), { once: true });
    } catch (e) {
      this._showSoulEditorError(e?.message || 'The model could not write a draft.');
    } finally {
      button.disabled = false;
      button.textContent = '✨ Draft';
    }
  },

  _renderMcpSection() {
    const section = this._createSection('mcp', 'MCP Servers');

    // Server list group
    const serverGroup = this._createGroup('Servers');

    const serverList = document.createElement('div');
    serverList.className = 'mcp-server-list';
    serverList.id = 'mcp-server-list';

    // Empty state shown when no servers
    const emptyState = document.createElement('div');
    emptyState.className = 'mcp-empty-state';
    emptyState.id = 'mcp-empty-state';
    const emptyTitle = document.createElement('p');
    emptyTitle.className = 'mcp-empty-title';
    emptyTitle.textContent = 'No MCP servers configured';
    const emptyHint = document.createElement('p');
    emptyHint.className = 'mcp-empty-hint';
    emptyHint.textContent =
      'Add a server to connect your agent to external tools and data sources.';
    emptyState.appendChild(emptyTitle);
    emptyState.appendChild(emptyHint);
    serverList.appendChild(emptyState);

    serverGroup.appendChild(serverList);

    const addBtn = document.createElement('button');
    addBtn.className = 'mcp-add-server-btn';
    addBtn.type = 'button';
    addBtn.textContent = '+ Add MCP Server';
    addBtn.addEventListener('click', () => this._openMcpModal());
    serverGroup.appendChild(addBtn);

    section.appendChild(serverGroup);

    // Global connection settings
    const globalGroup = this._createGroup('Connection Settings');
    globalGroup.appendChild(
      this._createToggleRow('Auto-reconnect', 'mcp.global.autoReconnect', true)
    );
    globalGroup.appendChild(
      this._createNumberRow(
        'Health check interval',
        'mcp.global.healthCheckInterval',
        '60',
        'Seconds between health checks'
      )
    );
    globalGroup.appendChild(
      this._createNumberRow('Max reconnect attempts', 'mcp.global.maxReconnectAttempts', '3', '')
    );
    globalGroup.appendChild(
      this._createNumberRow(
        'Reconnect delay',
        'mcp.global.reconnectDelay',
        '5',
        'Seconds between reconnect attempts'
      )
    );
    section.appendChild(globalGroup);

    return section;
  },

  _createServerItem(server, index) {
    const item = document.createElement('div');
    item.className = 'mcp-server-item';
    item.dataset.serverIndex = index;

    // Left: info
    const info = document.createElement('div');
    info.className = 'mcp-server-info';

    const name = document.createElement('div');
    name.className = 'mcp-server-name';
    name.textContent = server.name || 'Unnamed server';
    info.appendChild(name);

    const type = document.createElement('div');
    type.className = 'mcp-server-type';
    const serverType = server.type || 'stdio';
    let detail = '';
    if (serverType === 'stdio') {
      detail = server.command || '';
    } else {
      detail = server.url || '';
    }
    type.textContent = detail ? `${serverType} \u2022 ${detail}` : serverType;
    info.appendChild(type);

    item.appendChild(info);

    // Right: status
    const status = document.createElement('div');
    status.className = 'mcp-server-status';

    const badge = document.createElement('span');
    const isActive = server.enabled !== false;
    badge.className = `mcp-status-badge ${isActive ? 'active' : 'inactive'}`;
    badge.textContent = isActive ? 'Active' : 'Inactive';
    status.appendChild(badge);

    // Toggle
    const toggle = document.createElement('label');
    toggle.className = 'mcp-toggle';
    const toggleInput = document.createElement('input');
    toggleInput.type = 'checkbox';
    toggleInput.checked = isActive;
    toggleInput.addEventListener('change', async () => {
      const srv = (this._mcpServers || [])[index];
      if (srv) {
        const updatedServer = { ...srv, enabled: toggleInput.checked };
        try {
          await this._sendMcpCommand('mcp.update', { name: srv.name, server: updatedServer });
          srv.enabled = toggleInput.checked;
          badge.className = `mcp-status-badge ${toggleInput.checked ? 'active' : 'inactive'}`;
          badge.textContent = toggleInput.checked ? 'Active' : 'Inactive';
        } catch (e) {
          console.error('Failed to toggle MCP server:', e);
          toggleInput.checked = !toggleInput.checked;
        }
      }
    });
    const toggleSlider = document.createElement('span');
    toggleSlider.className = 'mcp-toggle-slider';
    toggle.appendChild(toggleInput);
    toggle.appendChild(toggleSlider);
    status.appendChild(toggle);

    // Edit button
    const editBtn = document.createElement('button');
    editBtn.className = 'mcp-icon-btn';
    editBtn.type = 'button';
    editBtn.textContent = '\u270E';
    editBtn.title = 'Edit server';
    editBtn.addEventListener('click', () => this._openMcpModal(server, index));
    status.appendChild(editBtn);

    // Delete button
    const deleteBtn = document.createElement('button');
    deleteBtn.className = 'mcp-icon-btn delete';
    deleteBtn.type = 'button';
    deleteBtn.textContent = '\u2715';
    deleteBtn.title = 'Remove server';
    deleteBtn.addEventListener('click', async () => {
      if (!confirm(`Remove server "${server.name || 'Unnamed'}"?`)) return;
      try {
        await this._sendMcpCommand('mcp.delete', { name: server.name });
        await this._populateMcpServers();
      } catch (e) {
        console.error('Failed to delete MCP server:', e);
      }
    });
    status.appendChild(deleteBtn);

    item.appendChild(status);
    return item;
  },

  _ensureMcpModal() {
    if (this._mcpModal) return;

    const modal = document.createElement('div');
    modal.className = 'mcp-modal';
    modal.id = 'mcp-modal';
    modal.addEventListener('click', (e) => {
      if (e.target === modal) this._closeMcpModal();
    });

    const content = document.createElement('div');
    content.className = 'mcp-modal-content';

    // Header
    const header = document.createElement('div');
    header.className = 'mcp-modal-header';
    const title = document.createElement('h2');
    title.id = 'mcp-modal-title';
    title.textContent = 'Add MCP Server';
    const subtitle = document.createElement('p');
    subtitle.textContent = 'Configure a Model Context Protocol server';
    header.appendChild(title);
    header.appendChild(subtitle);
    content.appendChild(header);

    // Form
    const form = document.createElement('div');
    form.id = 'mcp-modal-form';

    // Server Name
    form.appendChild(
      this._createModalFormGroup(
        'Server Name *',
        'mcp-modal-name',
        'text',
        'e.g., my-server',
        'A unique identifier for this server'
      )
    );

    // Connection Type
    const typeGroup = document.createElement('div');
    typeGroup.className = 'mcp-form-group';
    const typeLabel = document.createElement('label');
    typeLabel.className = 'mcp-form-label';
    typeLabel.textContent = 'Connection Type *';
    typeGroup.appendChild(typeLabel);
    const typeSelect = document.createElement('select');
    typeSelect.className = 'mcp-form-select';
    typeSelect.id = 'mcp-modal-type';
    for (const [val, text] of [
      ['stdio', 'stdio - Standard I/O Process'],
      ['sse', 'sse - Server-Sent Events'],
      ['http', 'http - HTTP/REST API'],
    ]) {
      const opt = document.createElement('option');
      opt.value = val;
      opt.textContent = text;
      typeSelect.appendChild(opt);
    }
    typeSelect.addEventListener('change', () => this._updateMcpTypeFields());
    typeGroup.appendChild(typeSelect);
    const typeHelp = document.createElement('div');
    typeHelp.className = 'mcp-form-help';
    typeHelp.textContent = 'How the MCP server communicates';
    typeGroup.appendChild(typeHelp);
    form.appendChild(typeGroup);

    // ── stdio fields ──
    const stdioFields = document.createElement('div');
    stdioFields.className = 'mcp-conditional-fields';
    stdioFields.id = 'mcp-fields-stdio';
    stdioFields.appendChild(
      this._createModalFormGroup(
        'Command *',
        'mcp-modal-command',
        'text',
        'npx -y @modelcontextprotocol/server-name',
        'Command to start the server process'
      )
    );
    stdioFields.appendChild(
      this._createModalFormGroup(
        'Arguments',
        'mcp-modal-args',
        'text',
        '--arg1 value1 --arg2 value2',
        'Space-separated command arguments'
      )
    );
    const stdioRow = document.createElement('div');
    stdioRow.className = 'mcp-form-row';
    const workDirGroup = this._createModalFormGroup(
      'Working Directory',
      'mcp-modal-workdir',
      'text',
      '/path/to/dir'
    );
    const envGroup = this._createModalFormGroup(
      'Environment',
      'mcp-modal-env',
      'text',
      'KEY=value,KEY2=value2'
    );
    stdioRow.appendChild(workDirGroup);
    stdioRow.appendChild(envGroup);
    stdioFields.appendChild(stdioRow);
    form.appendChild(stdioFields);

    // ── sse fields ──
    const sseFields = document.createElement('div');
    sseFields.className = 'mcp-conditional-fields';
    sseFields.id = 'mcp-fields-sse';
    sseFields.style.display = 'none';
    sseFields.appendChild(
      this._createModalFormGroup(
        'SSE Endpoint URL *',
        'mcp-modal-sse-url',
        'url',
        'http://localhost:3000/events',
        'Server-Sent Events endpoint URL'
      )
    );
    const sseRow = document.createElement('div');
    sseRow.className = 'mcp-form-row';
    sseRow.appendChild(
      this._createModalFormGroup('Reconnect (ms)', 'mcp-modal-sse-reconnect', 'number', '5000')
    );
    sseRow.appendChild(
      this._createModalFormGroup('Timeout (ms)', 'mcp-modal-sse-timeout', 'number', '30000')
    );
    sseFields.appendChild(sseRow);
    sseFields.appendChild(
      this._createModalFormGroup(
        'Headers (JSON)',
        'mcp-modal-sse-headers',
        'text',
        '{"Authorization": "Bearer token"}'
      )
    );
    form.appendChild(sseFields);

    // ── http fields ──
    const httpFields = document.createElement('div');
    httpFields.className = 'mcp-conditional-fields';
    httpFields.id = 'mcp-fields-http';
    httpFields.style.display = 'none';
    httpFields.appendChild(
      this._createModalFormGroup(
        'Base URL *',
        'mcp-modal-http-url',
        'url',
        'https://api.example.com/mcp',
        'Base URL for the HTTP API'
      )
    );
    const httpRow = document.createElement('div');
    httpRow.className = 'mcp-form-row';
    // Method select
    const methodGroup = document.createElement('div');
    methodGroup.className = 'mcp-form-group';
    const methodLabel = document.createElement('label');
    methodLabel.className = 'mcp-form-label';
    methodLabel.textContent = 'Request Method';
    methodGroup.appendChild(methodLabel);
    const methodSelect = document.createElement('select');
    methodSelect.className = 'mcp-form-select';
    methodSelect.id = 'mcp-modal-http-method';
    for (const m of ['POST', 'GET', 'PUT']) {
      const opt = document.createElement('option');
      opt.value = m;
      opt.textContent = m;
      methodSelect.appendChild(opt);
    }
    methodGroup.appendChild(methodSelect);
    httpRow.appendChild(methodGroup);
    httpRow.appendChild(
      this._createModalFormGroup('Timeout (ms)', 'mcp-modal-http-timeout', 'number', '30000')
    );
    httpFields.appendChild(httpRow);
    httpFields.appendChild(
      this._createModalFormGroup(
        'Headers (JSON)',
        'mcp-modal-http-headers',
        'text',
        '{"Content-Type": "application/json"}'
      )
    );
    httpFields.appendChild(
      this._createModalFormGroup('API Key', 'mcp-modal-http-apikey', 'password', 'Enter API key')
    );
    form.appendChild(httpFields);

    // Description
    form.appendChild(
      this._createModalFormGroup(
        'Description',
        'mcp-modal-desc',
        'text',
        'What this server does...'
      )
    );

    content.appendChild(form);

    // Actions
    const actions = document.createElement('div');
    actions.className = 'mcp-modal-actions';
    const cancelBtn = document.createElement('button');
    cancelBtn.className = 'mcp-btn-secondary';
    cancelBtn.type = 'button';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.addEventListener('click', () => this._closeMcpModal());
    const saveBtn = document.createElement('button');
    saveBtn.className = 'mcp-btn-primary';
    saveBtn.type = 'button';
    saveBtn.textContent = 'Save Server';
    saveBtn.addEventListener('click', () => this._saveMcpServer());
    actions.appendChild(cancelBtn);
    actions.appendChild(saveBtn);
    content.appendChild(actions);

    modal.appendChild(content);
    document.body.appendChild(modal);
    this._mcpModal = modal;
  },

  _createModalFormGroup(label, id, type, placeholder, helpText) {
    const group = document.createElement('div');
    group.className = 'mcp-form-group';

    const lbl = document.createElement('label');
    lbl.className = 'mcp-form-label';
    lbl.textContent = label;
    group.appendChild(lbl);

    const input = document.createElement('input');
    input.className = 'mcp-form-input';
    input.type = type || 'text';
    input.id = id;
    input.placeholder = placeholder || '';
    group.appendChild(input);

    if (helpText) {
      const help = document.createElement('div');
      help.className = 'mcp-form-help';
      help.textContent = helpText;
      group.appendChild(help);
    }

    return group;
  },

  _openMcpModal(server, index) {
    this._ensureMcpModal();
    this._mcpEditIndex = index !== undefined ? index : -1;

    const title = document.getElementById('mcp-modal-title');
    title.textContent = server ? 'Edit MCP Server' : 'Add MCP Server';

    // Reset all fields
    const ids = [
      'mcp-modal-name',
      'mcp-modal-command',
      'mcp-modal-args',
      'mcp-modal-workdir',
      'mcp-modal-env',
      'mcp-modal-sse-url',
      'mcp-modal-sse-reconnect',
      'mcp-modal-sse-timeout',
      'mcp-modal-sse-headers',
      'mcp-modal-http-url',
      'mcp-modal-http-timeout',
      'mcp-modal-http-headers',
      'mcp-modal-http-apikey',
      'mcp-modal-desc',
    ];
    for (const id of ids) {
      const el = document.getElementById(id);
      if (el) el.value = '';
    }
    document.getElementById('mcp-modal-type').value = 'stdio';
    document.getElementById('mcp-modal-http-method').value = 'POST';

    // Populate if editing
    if (server) {
      document.getElementById('mcp-modal-name').value = server.name || '';
      document.getElementById('mcp-modal-desc').value = server.description || '';

      const serverType = server.type || 'stdio';
      document.getElementById('mcp-modal-type').value = serverType;

      if (serverType === 'stdio') {
        document.getElementById('mcp-modal-command').value = server.command || '';
        document.getElementById('mcp-modal-args').value = Array.isArray(server.args)
          ? server.args.join(' ')
          : server.args || '';
        document.getElementById('mcp-modal-workdir').value = server.work_dir || '';
        // Env: object → comma-separated KEY=value
        if (server.env && typeof server.env === 'object') {
          document.getElementById('mcp-modal-env').value = Object.entries(server.env)
            .map(([k, v]) => `${k}=${v}`)
            .join(',');
        }
      } else if (serverType === 'sse') {
        document.getElementById('mcp-modal-sse-url').value = server.url || '';
        document.getElementById('mcp-modal-sse-reconnect').value = server.reconnect || '';
        document.getElementById('mcp-modal-sse-timeout').value = server.timeout || '';
        if (server.headers) {
          document.getElementById('mcp-modal-sse-headers').value =
            typeof server.headers === 'string' ? server.headers : JSON.stringify(server.headers);
        }
      } else if (serverType === 'http') {
        document.getElementById('mcp-modal-http-url').value = server.url || '';
        document.getElementById('mcp-modal-http-method').value = server.method || 'POST';
        document.getElementById('mcp-modal-http-timeout').value = server.timeout || '';
        if (server.headers) {
          document.getElementById('mcp-modal-http-headers').value =
            typeof server.headers === 'string' ? server.headers : JSON.stringify(server.headers);
        }
        document.getElementById('mcp-modal-http-apikey').value = server.api_key || '';
      }
    }

    this._updateMcpTypeFields();
    this._mcpModal.classList.add('show');

    // Focus name input
    setTimeout(() => document.getElementById('mcp-modal-name').focus(), 50);
  },

  _closeMcpModal() {
    if (this._mcpModal) {
      this._mcpModal.classList.remove('show');
    }
  },

  _updateMcpTypeFields() {
    const type = document.getElementById('mcp-modal-type').value;
    const stdio = document.getElementById('mcp-fields-stdio');
    const sse = document.getElementById('mcp-fields-sse');
    const http = document.getElementById('mcp-fields-http');
    if (stdio) stdio.style.display = type === 'stdio' ? '' : 'none';
    if (sse) sse.style.display = type === 'sse' ? '' : 'none';
    if (http) http.style.display = type === 'http' ? '' : 'none';
  },

  async _saveMcpServer() {
    const name = document.getElementById('mcp-modal-name').value.trim();
    const type = document.getElementById('mcp-modal-type').value;

    if (!name) {
      document.getElementById('mcp-modal-name').focus();
      return;
    }

    const server = {
      name,
      type,
      enabled: true,
      description: document.getElementById('mcp-modal-desc').value.trim() || undefined,
    };

    if (type === 'stdio') {
      server.command = document.getElementById('mcp-modal-command').value.trim();
      if (!server.command) {
        document.getElementById('mcp-modal-command').focus();
        return;
      }
      const argsStr = document.getElementById('mcp-modal-args').value.trim();
      server.args = argsStr ? argsStr.split(/\s+/) : [];
      server.work_dir = document.getElementById('mcp-modal-workdir').value.trim() || undefined;
      const envStr = document.getElementById('mcp-modal-env').value.trim();
      if (envStr) {
        server.env = {};
        for (const pair of envStr.split(',')) {
          const eqIdx = pair.indexOf('=');
          if (eqIdx > 0) {
            server.env[pair.slice(0, eqIdx).trim()] = pair.slice(eqIdx + 1).trim();
          }
        }
      } else {
        server.env = {};
      }
    } else if (type === 'sse') {
      server.url = document.getElementById('mcp-modal-sse-url').value.trim();
      if (!server.url) {
        document.getElementById('mcp-modal-sse-url').focus();
        return;
      }
      const reconnect = document.getElementById('mcp-modal-sse-reconnect').value;
      if (reconnect) server.reconnect = parseInt(reconnect, 10);
      const timeout = document.getElementById('mcp-modal-sse-timeout').value;
      if (timeout) server.timeout = parseInt(timeout, 10);
      const headersStr = document.getElementById('mcp-modal-sse-headers').value.trim();
      if (headersStr) {
        try {
          server.headers = JSON.parse(headersStr);
        } catch {
          /* ignore parse error */
        }
      }
    } else if (type === 'http') {
      server.url = document.getElementById('mcp-modal-http-url').value.trim();
      if (!server.url) {
        document.getElementById('mcp-modal-http-url').focus();
        return;
      }
      server.method = document.getElementById('mcp-modal-http-method').value;
      const timeout = document.getElementById('mcp-modal-http-timeout').value;
      if (timeout) server.timeout = parseInt(timeout, 10);
      const headersStr = document.getElementById('mcp-modal-http-headers').value.trim();
      if (headersStr) {
        try {
          server.headers = JSON.parse(headersStr);
        } catch {
          /* ignore parse error */
        }
      }
      const apiKey = document.getElementById('mcp-modal-http-apikey').value.trim();
      if (apiKey) server.api_key = apiKey;
    }

    try {
      if (this._mcpEditIndex >= 0 && this._mcpEditIndex < (this._mcpServers || []).length) {
        const oldName = this._mcpServers[this._mcpEditIndex].name;
        server.enabled = this._mcpServers[this._mcpEditIndex].enabled !== false;
        await this._sendMcpCommand('mcp.update', { name: oldName, server });
      } else {
        await this._sendMcpCommand('mcp.add', { server });
      }
      await this._populateMcpServers();
    } catch (e) {
      console.error('Failed to save MCP server:', e);
    }
    this._closeMcpModal();
  },

  _refreshMcpServerList() {
    const list = document.getElementById('mcp-server-list');
    if (!list) return;

    // Remove existing server items (keep empty state)
    const items = list.querySelectorAll('.mcp-server-item');
    for (const item of items) item.remove();

    const servers = this._mcpServers || [];
    for (let i = 0; i < servers.length; i++) {
      list.appendChild(this._createServerItem(servers[i], i));
    }

    this._updateMcpEmptyState();
  },

  _updateMcpEmptyState() {
    const list = document.getElementById('mcp-server-list');
    const emptyState = document.getElementById('mcp-empty-state');
    if (!emptyState) return;
    const hasItems = list.querySelector('.mcp-server-item');
    emptyState.style.display = hasItems ? 'none' : '';
  },

  async _sendAgentCommand(command, params = {}, timeoutMs) {
    return this._sendMcpCommand(command, params, timeoutMs);
  },

  async _retryWithBackoff(fn, delays = [1000, 3000, 5000, 10000, 15000]) {
    for (let i = 0; i < delays.length; i++) {
      await new Promise(resolve => setTimeout(resolve, delays[i]));
      try {
        return await fn();
      } catch (e) {
        if (i === delays.length - 1) throw e;
      }
    }
  },

  async _getStatusCache() {
    try {
      const result = await NevofluxPage.sendQuery('bridge:request', {
        type: 'getCache',
        payload: { key: 'nevoflux_last_status' },
      });
      return result.success ? result.data : null;
    } catch (e) {
      return null;
    }
  },

  async _queryAgentStatus() {
    return this._sendAgentCommand('status');
  },

  _showLlmConnecting(message) {
    const grid = document.getElementById('llm-providers-grid');
    if (!grid) return;
    grid.innerHTML = '';

    const div = document.createElement('div');
    div.className = 'llm-connecting';
    div.id = 'llm-connecting';

    const spinner = document.createElement('div');
    spinner.className = 'llm-connecting-spinner';
    div.appendChild(spinner);

    const text = document.createElement('div');
    text.className = 'llm-connecting-text';
    text.textContent = message;
    div.appendChild(text);

    grid.appendChild(div);
  },

  _showLlmError(message, retryFn) {
    const grid = document.getElementById('llm-providers-grid');
    if (!grid) return;
    grid.innerHTML = '';

    const div = document.createElement('div');
    div.className = 'llm-error';

    const text = document.createElement('div');
    text.className = 'llm-error-text';
    text.textContent = message;
    div.appendChild(text);

    const btn = document.createElement('button');
    btn.className = 'llm-retry-button';
    btn.textContent = 'Retry';
    btn.addEventListener('click', () => retryFn());
    div.appendChild(btn);

    grid.appendChild(div);
  },

  _showLlmSkeleton() {
    const grid = document.getElementById('llm-providers-grid');
    if (!grid) return;
    grid.innerHTML = '';

    for (let i = 0; i < 6; i++) {
      const card = document.createElement('div');
      card.className = 'llm-provider-card skeleton';

      const icon = document.createElement('div');
      icon.className = 'skeleton-icon';
      card.appendChild(icon);

      const textWrap = document.createElement('div');
      textWrap.className = 'skeleton-text';

      const line1 = document.createElement('div');
      line1.className = 'skeleton-line wide';
      textWrap.appendChild(line1);

      const line2 = document.createElement('div');
      line2.className = 'skeleton-line narrow';
      textWrap.appendChild(line2);

      card.appendChild(textWrap);
      grid.appendChild(card);
    }
  },

  _showGuidanceBanner(text) {
    const banner = document.getElementById('llm-guidance-banner');
    if (!banner) return;
    banner.textContent = text;
    banner.style.display = 'block';
  },

  _hideGuidanceBanner() {
    const banner = document.getElementById('llm-guidance-banner');
    if (banner) banner.style.display = 'none';
  },

  async _sendMcpCommand(command, params = {}, timeoutMs) {
    // NevofluxParent wraps the result: { success, data: <bridgeRespond payload> }
    const result = await NevofluxPage.sendQuery('bridge:request', {
      type: 'agent:command',
      payload: { command, params },
      timeoutMs,
    });
    if (!result.success) {
      throw new Error(result.error?.message || 'Bridge request failed');
    }
    // result.data is the agent system_response payload: { request_id, command, success, data }
    const agentResponse = result.data;
    if (!agentResponse.success) {
      throw new Error(agentResponse.error?.message || 'Agent command failed');
    }
    return agentResponse.data;
  },

  _createToggleRow(label, key, defaultValue) {
    const row = document.createElement('div');
    row.className = 'toggle-row';

    const lbl = document.createElement('label');
    lbl.textContent = label;
    row.appendChild(lbl);

    const toggle = document.createElement('label');
    toggle.className = 'mcp-toggle';

    const input = document.createElement('input');
    input.type = 'checkbox';
    input.checked = defaultValue;
    input.dataset.key = key;
    input.addEventListener('change', () => this._onFieldChange(key, input.checked));

    const slider = document.createElement('span');
    slider.className = 'mcp-toggle-slider';

    toggle.appendChild(input);
    toggle.appendChild(slider);
    row.appendChild(toggle);

    return row;
  },

  _createNumberRow(label, key, placeholder, hint) {
    const row = document.createElement('div');
    row.className = 'form-row';

    const lbl = document.createElement('label');
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement('div');
    field.className = 'field';

    const input = document.createElement('input');
    input.type = 'number';
    input.min = '0';
    input.placeholder = placeholder || '';
    input.dataset.key = key;
    input.addEventListener('input', () => {
      const val = input.value ? parseInt(input.value, 10) : undefined;
      if (val !== undefined && !isNaN(val)) {
        this._onFieldChange(key, val);
      }
    });
    field.appendChild(input);

    if (hint) {
      const hintEl = document.createElement('div');
      hintEl.className = 'hint';
      hintEl.textContent = hint;
      field.appendChild(hintEl);
    }

    row.appendChild(field);
    return row;
  },

  // ── Form Helpers ────────────────────────────────────────

  _createSection(id, _title) {
    const section = document.createElement('div');
    section.className = 'settings-section';
    section.id = `section-${id}`;
    return section;
  },

  // ── Speech models (P0.5) ────────────────────────────────
  //
  // Voice needs about 240 MB before it can hear anything and another 120 MB
  // before it can answer out loud. Until now those arrived through a developer
  // recipe (`just fetch-asr-models`), which serves nobody without a checkout.
  //
  // Two tiers, downloaded separately and on request. The daemon does the
  // fetching (`models.status` / `models.download` / `models.cancel`) and
  // streams progress on `system:models:progress` — the same shape as the KB
  // install wizard, because it is the same problem: a long job whose progress
  // cannot fit in a request/response round trip.

  _speechModelTiers: [
    {
      id: 'transcribe',
      title: 'Speech input',
      desc: 'Recognises what you say. Voice input needs this.',
    },
    {
      id: 'speak',
      title: 'Spoken replies',
      desc: 'Reads answers aloud. English only — a small fallback voice.',
    },
    {
      id: 'speak-chinese',
      title: 'Chinese replies',
      desc:
        'Reads answers aloud in Chinese as well as English. Supersedes the ' +
        'tier above — it carries English voices too, so there is no reason ' +
        'to keep both.',
    },
    {
      id: 'speak-multilingual',
      title: 'Natural voice',
      desc:
        'Speaks Chinese and 19 other languages, and sounds markedly better. ' +
        'Large; the fallback above works without it.',
    },
  ],

  _renderSpeechModelsGroup() {
    const group = this._createGroup('Speech Models');

    const desc = document.createElement('p');
    desc.className = 'section-desc';
    desc.textContent =
      'Downloaded on request and kept in ~/.cache/nevoflux/models. ' +
      'Each file is checked against a pinned size and digest, and an ' +
      'interrupted download resumes where it stopped.';
    group.appendChild(desc);

    this._speechModelRows = {};
    for (const tier of this._speechModelTiers) {
      const row = document.createElement('div');
      row.className = 'speech-model-row';
      row.dataset.tier = tier.id;

      const head = document.createElement('div');
      head.className = 'speech-model-head';

      const label = document.createElement('div');
      label.className = 'speech-model-label';
      const name = document.createElement('span');
      name.className = 'speech-model-name';
      name.textContent = tier.title;
      const size = document.createElement('span');
      size.className = 'speech-model-size';
      label.append(name, size);

      const state = document.createElement('span');
      state.className = 'speech-model-state';
      state.textContent = 'Checking…';

      const button = document.createElement('button');
      button.className = 'btn-secondary speech-model-button';
      button.textContent = 'Download';
      button.disabled = true;
      button.addEventListener('click', () => this._speechModelButtonClicked(tier.id));

      head.append(label, state, button);

      const hint = document.createElement('p');
      hint.className = 'speech-model-hint';
      hint.textContent = tier.desc;

      const bar = document.createElement('div');
      bar.className = 'speech-model-bar';
      bar.hidden = true;
      const fill = document.createElement('div');
      fill.className = 'speech-model-fill';
      bar.appendChild(fill);

      row.append(head, hint, bar);
      group.appendChild(row);

      this._speechModelRows[tier.id] = { row, size, state, button, bar, fill, ready: false };
    }

    // Kick off after the section is mounted; the RPC is async and the group
    // has to exist before its rows can be filled in.
    setTimeout(() => {
      this._speechModelsRefresh();
      this._speechModelsEnsureSubscribed();
    }, 0);

    return group;
  },

  _speechModelBytes(n) {
    if (!n) return '0 MB';
    const mb = n / (1024 * 1024);
    return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
  },

  async _speechModelsRefresh() {
    let data;
    try {
      data = await this._sendMcpCommand('models.status', {});
    } catch (e) {
      // On a cold start the page is ready before the bridge is, and the first
      // call fails with "no handler registered". Giving up here leaves the
      // panel reading "Agent not reachable" for a daemon that came up two
      // seconds later — so retry before believing it.
      try {
        data = await this._retryWithBackoff(() => this._sendMcpCommand('models.status', {}));
      } catch (e2) {
        console.warn('[models] status unavailable:', e2);
        for (const id of Object.keys(this._speechModelRows || {})) {
          const r = this._speechModelRows[id];
          r.state.textContent = 'Agent not reachable';
          r.button.disabled = true;
        }
        return;
      }
    }
    for (const tier of data?.tiers || []) {
      const r = this._speechModelRows?.[tier.id];
      if (!r) continue;
      r.ready = !!tier.ready;
      r.size.textContent = this._speechModelBytes(tier.bytes);
      if (tier.downloading) {
        this._speechModelSetRunning(tier.id, tier.bytes - tier.remaining, tier.bytes);
      } else if (tier.ready) {
        r.state.textContent = 'Ready';
        r.button.textContent = 'Download';
        r.button.disabled = true;
        r.bar.hidden = true;
      } else {
        // A resumable partial is worth saying: the number the user sees next
        // time will start well above zero, and unexplained progress is its own
        // kind of confusing.
        const done = tier.bytes - tier.remaining;
        r.state.textContent =
          done > 0 ? `Paused at ${this._speechModelBytes(done)}` : 'Not downloaded';
        r.button.textContent = done > 0 ? 'Resume' : 'Download';
        r.button.disabled = false;
        r.bar.hidden = true;
      }
    }
  },

  _speechModelSetRunning(tierId, done, total) {
    const r = this._speechModelRows?.[tierId];
    if (!r) return;
    r.state.textContent = `${this._speechModelBytes(done)} of ${this._speechModelBytes(total)}`;
    r.button.textContent = 'Cancel';
    r.button.disabled = false;
    r.bar.hidden = false;
    r.fill.style.width = total ? `${Math.min(100, (done / total) * 100)}%` : '0%';
    r.running = true;
  },

  // Subscribe once, best effort, and try again whenever a download is about to
  // start. The subscription is what turns a download into visible progress; a
  // failed one at page load must not mean the next hour of downloading is
  // invisible.
  async _speechModelsEnsureSubscribed() {
    if (this._speechModelsSubscribed) return true;
    try {
      await this._speechModelsSubscribe();
      this._speechModelsSubscribed = true;
      return true;
    } catch (e) {
      console.warn('[models] progress subscribe failed:', e);
      return false;
    }
  },

  async _speechModelButtonClicked(tierId) {
    const r = this._speechModelRows?.[tierId];
    if (!r) return;
    const command = r.running ? 'models.cancel' : 'models.download';
    r.button.disabled = true;
    if (command === 'models.download') {
      const subscribed = await this._speechModelsEnsureSubscribed();
      if (!subscribed) {
        // Say it rather than showing a bar that will never move.
        r.state.title = 'Progress updates unavailable; the download still runs.';
      }
    }
    try {
      await this._sendMcpCommand(command, { tier: tierId });
      if (command === 'models.download') {
        this._speechModelSetRunning(tierId, 0, 0);
        r.state.textContent = 'Starting…';
      }
    } catch (e) {
      r.state.textContent = `Failed: ${e.message}`;
    } finally {
      r.button.disabled = false;
      if (command === 'models.cancel') {
        r.running = false;
        this._speechModelsRefresh();
      }
    }
  },

  _speechModelsOnProgress(frame) {
    const r = this._speechModelRows?.[frame?.tier];
    if (!r) return;
    if (frame.status === 'running') {
      this._speechModelSetRunning(frame.tier, frame.done, frame.total);
      return;
    }
    r.running = false;
    if (frame.status === 'failed') {
      // The daemon's message names every source it tried and what each said.
      // Truncating that to "download failed" would remove the only part a
      // user can act on.
      r.state.textContent = 'Failed';
      r.state.title = frame.message || '';
      r.button.textContent = 'Retry';
      r.button.disabled = false;
      r.bar.hidden = true;
      return;
    }
    this._speechModelsRefresh();
  },

  async _speechModelsSubscribe() {
    // Same two-step as the KB wizard: a persistent EventBus channel, then
    // subscribe through the bridge. The channel is what keeps frames arriving
    // past bridge:request's short push grace window.
    const channelId = 'models_' + Date.now() + '_' + Math.random().toString(36).slice(2, 8);
    await NevofluxPage.sendQuery('events:channel_open', { channelId });

    const listener = (event) => {
      const detail = event.detail;
      if (!detail || detail.type !== 'bridge:push') return;
      const msg = detail.msg;
      if (!msg || msg.type !== 'events:delivery') return;
      const ev = msg.payload?.event;
      if (!ev || ev.topic !== 'system:models:progress') return;
      try {
        this._speechModelsOnProgress(ev.payload);
      } catch (err) {
        console.warn('[models] progress handler failed:', err);
      }
    };
    window.addEventListener('NevofluxMessage', listener);

    const res = await NevofluxPage.sendQuery('bridge:request', {
      type: 'events.subscribe',
      payload: {
        patterns: ['system:models:progress'],
        replay_sticky: false,
        channel_id: channelId,
      },
    });
    if (!res || res.success === false) {
      window.removeEventListener('NevofluxMessage', listener);
      try {
        await NevofluxPage.sendQuery('events:channel_close', { channelId });
      } catch (_e) {}
      throw new Error(res?.error?.message || 'events.subscribe failed');
    }
  },

  // ── Voice (P1) ──────────────────────────────────────────
  //
  // The list comes from the daemon (`speech.voices`), not from here: which
  // voices exist depends on which engine will actually speak, and offering
  // MOSS's eighteen while Kokoro is the one running lets someone pick a voice
  // that silently does nothing.

  _renderSpeechVoiceGroup() {
    const group = this._createGroup('Voice');

    const engineRow = document.createElement('div');
    engineRow.className = 'speech-model-head';

    const line = document.createElement('p');
    line.className = 'section-desc';
    line.id = 'speech-engine-line';
    line.style.flex = '1';
    line.style.margin = '0';
    line.textContent = 'Checking which engine will speak…';

    // The way out of a bad measurement. Without it, a machine judged too slow
    // once never runs the engine again, so it never measures again, and the
    // only escape was editing config.toml by hand.
    const remeasure = document.createElement('button');
    remeasure.className = 'btn-secondary';
    remeasure.id = 'speech-remeasure';
    remeasure.textContent = 'Re-measure';
    remeasure.hidden = true;
    remeasure.addEventListener('click', async () => {
      remeasure.disabled = true;
      try {
        await this._sendMcpCommand('speech.reset_rtf', {});
        line.textContent = 'Cleared — the next reply will be spoken by the multilingual engine.';
      } catch (e) {
        line.textContent = `Could not clear the measurement: ${e.message}`;
      } finally {
        remeasure.disabled = false;
      }
    });

    engineRow.append(line, remeasure);
    group.appendChild(engineRow);

    const row = document.createElement('div');
    row.className = 'form-row';
    const label = document.createElement('label');
    label.textContent = 'Voice';
    const select = document.createElement('select');
    select.id = 'speech-voice-select';
    select.disabled = true;
    select.addEventListener('change', () =>
      this._onFieldChange('general.speechVoice', select.value)
    );
    row.append(label, select);
    group.appendChild(row);

    // §5.2:纯渲染层开关。正文照常生成、入库、可搜索,只是当前会话不显示气泡
    // —— 所以它是偏好,不是模式,与 hands-free / tap-to-talk 正交。
    // `_createToggleRow` takes no hint argument; passing one would have been
    // silently dropped, so the explanation is its own line.
    // Speaking is a separate decision from listening. It used to ride on the
    // microphone: turn voice input on and every reply was read out, with no
    // middle setting — and wanting to talk to it is not the same as wanting
    // it to talk back. Off by default, because sound is an interruption.
    group.appendChild(this._createToggleRow('Speak replies', 'general.speakReplies', false));
    const speakHint = document.createElement('p');
    speakHint.className = 'section-desc';
    speakHint.textContent =
      'Reads answers out loud while voice input is on. Off, they only appear ' +
      'as text — dictation still works.';
    group.appendChild(speakHint);

    // On by default: the useful direction of this switch is off. Leaving it on
    // does not force the GPU — the engine still measures both and takes the
    // faster one, and a GPU is not always faster for these models. Turning it
    // off pins the CPU, which is the way back when a driver misbehaves.
    group.appendChild(this._createToggleRow('Use GPU when it helps', 'general.speechUseGpu', true));
    const gpuHint = document.createElement('p');
    gpuHint.className = 'section-desc';
    gpuHint.textContent =
      'Lets speech try DirectML or CUDA and keep whichever is faster than the ' +
      'CPU here. Turn off to stay on the CPU. Takes effect next time speech starts.';
    group.appendChild(gpuHint);

    group.appendChild(this._createToggleRow('Voice view', 'general.voiceView', false));
    const viewHint = document.createElement('p');
    viewHint.className = 'section-desc';
    viewHint.textContent =
      'Replaces message bubbles with a waveform while voice is on. Replies are ' +
      'still generated, saved and searchable — only this view changes.';
    group.appendChild(viewHint);

    setTimeout(() => this._speechVoicesRefresh(), 0);
    return group;
  },

  async _speechVoicesRefresh() {
    const line = document.getElementById('speech-engine-line');
    const select = document.getElementById('speech-voice-select');
    if (!line || !select) return;

    let data;
    try {
      data = await this._retryWithBackoff(() => this._sendMcpCommand('speech.voices', {}));
    } catch (e) {
      line.textContent = 'Agent not reachable; voices unavailable.';
      return;
    }

    // The reason is the whole point of showing this at all: for an English
    // speaker the fallback is a different voice, for a Chinese speaker it is
    // no voice, and neither should have to guess which engine they got.
    // Show the samples behind the verdict, not just the verdict: a single
    // number invites "that can't be right", and the spread answers it.
    const samples = Array.isArray(data.recent_rtf) ? data.recent_rtf : [];
    const spread = samples.length > 1 ? ` from ${samples.map((v) => v.toFixed(2)).join(', ')}` : '';
    const rtf =
      typeof data.measured_rtf === 'number'
        ? ` (${data.measured_rtf.toFixed(2)}x real time${spread})`
        : '';
    if (data.engine === 'moss') {
      line.textContent = `Speaking with MOSS — multilingual${rtf}.`;
    } else if (data.engine === 'kokoro') {
      // Which languages Kokoro speaks depends on which release is loaded, so
      // read it off the voices that are actually there rather than asserting
      // it here. v1.1-zh ships zf_/zm_ speakers and handles Chinese; v1.0 is
      // English only. Hardcoding "English only" was true until it wasn't, and
      // this line is exactly what someone reads to work out why they are
      // hearing English.
      const speaksChinese = (data.voices || []).some((v) =>
        /^z[fm]_/.test(String(v.id || v))
      );
      const langs = speaksChinese ? 'Chinese and English' : 'English only';
      line.textContent = `Speaking with Kokoro, ${langs}. ${data.reason || ''}`.trim();
    } else {
      line.textContent = data.reason || 'No speech engine is available.';
    }
    // Only worth offering when a measurement is what is holding it back.
    const remeasure = document.getElementById('speech-remeasure');
    if (remeasure) remeasure.hidden = typeof data.measured_rtf !== 'number';

    select.textContent = '';
    const voices = data.voices || [];
    if (!voices.length) {
      select.disabled = true;
      const opt = document.createElement('option');
      opt.textContent = 'No voices available';
      select.appendChild(opt);
      return;
    }
    // Grouped, because eighteen voices in one flat list is a wall of names.
    const groups = new Map();
    for (const v of voices) {
      const key = v.group || 'Voices';
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(v);
    }
    for (const [name, members] of groups) {
      const og = document.createElement('optgroup');
      og.label = name;
      for (const v of members) {
        const opt = document.createElement('option');
        opt.value = v.id;
        opt.textContent = v.name || v.id;
        og.appendChild(opt);
      }
      select.appendChild(og);
    }
    select.disabled = false;
    const current = this._settings?.general?.speechVoice;
    if (current && voices.some((v) => v.id === current)) select.value = current;
  },

  _createGroup(title) {
    const group = document.createElement('div');
    group.className = 'settings-group';
    const h2 = document.createElement('h2');
    h2.textContent = title;
    group.appendChild(h2);
    return group;
  },

  _createTextRow(label, key, placeholder, hint) {
    const row = document.createElement('div');
    row.className = 'form-row';

    const lbl = document.createElement('label');
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement('div');
    field.className = 'field';

    const input = document.createElement('input');
    input.type = 'text';
    input.placeholder = placeholder || '';
    input.dataset.key = key;
    input.addEventListener('input', () => this._onFieldChange(key, input.value));
    field.appendChild(input);

    if (hint) {
      const hintEl = document.createElement('div');
      hintEl.className = 'hint';
      hintEl.textContent = hint;
      field.appendChild(hintEl);
    }

    row.appendChild(field);
    return row;
  },

  _createTextareaRow(label, key, placeholder, hint) {
    const row = document.createElement('div');
    row.className = 'form-row';

    const lbl = document.createElement('label');
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement('div');
    field.className = 'field';

    const textarea = document.createElement('textarea');
    textarea.placeholder = placeholder || '';
    textarea.dataset.key = key;
    textarea.addEventListener('input', () => this._onFieldChange(key, textarea.value));
    field.appendChild(textarea);

    if (hint) {
      const hintEl = document.createElement('div');
      hintEl.className = 'hint';
      hintEl.textContent = hint;
      field.appendChild(hintEl);
    }

    row.appendChild(field);
    return row;
  },

  _createSelectRow(label, key, options, defaultValue) {
    const row = document.createElement('div');
    row.className = 'form-row';

    const lbl = document.createElement('label');
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement('div');
    field.className = 'field';

    const select = document.createElement('select');
    select.dataset.key = key;
    for (const [value, text] of options) {
      const opt = document.createElement('option');
      opt.value = value;
      opt.textContent = text;
      if (value === defaultValue) opt.selected = true;
      select.appendChild(opt);
    }
    select.addEventListener('change', () => this._onFieldChange(key, select.value));
    field.appendChild(select);

    row.appendChild(field);
    return row;
  },

  _createPasswordRow(label, key, placeholder) {
    const row = document.createElement('div');
    row.className = 'form-row';

    const lbl = document.createElement('label');
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement('div');
    field.className = 'field';

    const wrapper = document.createElement('div');
    wrapper.className = 'password-wrapper';

    const input = document.createElement('input');
    input.type = 'password';
    input.placeholder = placeholder || '';
    input.dataset.key = key;
    input.addEventListener('input', () => this._onFieldChange(key, input.value));

    const toggle = document.createElement('button');
    toggle.className = 'password-toggle';
    toggle.textContent = 'Show';
    toggle.type = 'button';
    toggle.addEventListener('click', () => {
      if (input.type === 'password') {
        input.type = 'text';
        toggle.textContent = 'Hide';
      } else {
        input.type = 'password';
        toggle.textContent = 'Show';
      }
    });

    wrapper.appendChild(input);
    wrapper.appendChild(toggle);
    field.appendChild(wrapper);
    row.appendChild(field);
    return row;
  },

  _createAvatarRow(label, key) {
    const row = document.createElement('div');
    row.className = 'avatar-row';

    const lbl = document.createElement('label');
    lbl.textContent = label;
    row.appendChild(lbl);

    const preview = document.createElement('div');
    preview.className = 'avatar-preview';
    preview.id = 'avatar-preview';
    preview.textContent = '?';
    row.appendChild(preview);

    const actions = document.createElement('div');
    actions.className = 'avatar-actions';

    const fileInput = document.createElement('input');
    fileInput.type = 'file';
    fileInput.accept = 'image/*';
    fileInput.style.display = 'none';
    fileInput.addEventListener('change', () => {
      const file = fileInput.files[0];
      if (!file) return;
      console.log('[Settings] Avatar file selected:', file.name, file.size, file.type);
      // Resize image to avatar size (256x256 max) using canvas
      this._resizeImage(file, 256, (dataUrl) => {
        console.log('[Settings] Avatar resized, dataUrl length:', dataUrl.length);
        this._setAvatarPreview(preview, dataUrl);
        this._onFieldChange(key, dataUrl);
      });
      // Reset so the same file can be re-selected
      fileInput.value = '';
    });
    actions.appendChild(fileInput);

    const uploadBtn = document.createElement('button');
    uploadBtn.type = 'button';
    uploadBtn.textContent = 'Upload';
    uploadBtn.addEventListener('click', () => fileInput.click());
    actions.appendChild(uploadBtn);

    const removeBtn = document.createElement('button');
    removeBtn.type = 'button';
    removeBtn.className = 'avatar-remove-btn';
    removeBtn.textContent = 'Remove';
    removeBtn.addEventListener('click', () => {
      this._setAvatarPreview(preview, null);
      this._onFieldChange(key, '');
    });
    actions.appendChild(removeBtn);

    row.appendChild(actions);
    row.dataset.key = key;
    row.dataset.avatarRow = 'true';
    return row;
  },

  _resizeImage(file, maxSize, callback) {
    const reader = new FileReader();
    reader.onload = () => {
      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement('canvas');
        let w = img.width;
        let h = img.height;
        // Scale down to fit within maxSize x maxSize
        if (w > maxSize || h > maxSize) {
          const ratio = Math.min(maxSize / w, maxSize / h);
          w = Math.round(w * ratio);
          h = Math.round(h * ratio);
        }
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext('2d');
        ctx.drawImage(img, 0, 0, w, h);
        // Use JPEG for photos (smaller), PNG for transparency
        const isPng = file.type === 'image/png';
        const mimeType = isPng ? 'image/png' : 'image/jpeg';
        const quality = isPng ? undefined : 0.85;
        const dataUrl = canvas.toDataURL(mimeType, quality);
        callback(dataUrl);
      };
      img.onerror = () => {
        console.error('[Settings] Failed to load image for resize');
      };
      img.src = reader.result;
    };
    reader.onerror = () => {
      console.error('[Settings] Failed to read avatar file:', reader.error);
    };
    reader.readAsDataURL(file);
  },

  _setAvatarPreview(preview, dataUrl) {
    if (dataUrl) {
      preview.textContent = '';
      let img = preview.querySelector('img');
      if (!img) {
        img = document.createElement('img');
        img.alt = 'Avatar';
        preview.appendChild(img);
      }
      img.src = dataUrl;
    } else {
      preview.textContent = '?';
      const img = preview.querySelector('img');
      if (img) img.remove();
    }
  },

  // ── Canvas Tools Section ────────────────────────────────

  _canvasTools: [],

  _renderCanvasToolsSection() {
    const section = this._createSection('canvas-tools', 'Canvas Tools');

    const group = this._createGroup('Available Tools');

    const desc = document.createElement('p');
    desc.className = 'section-desc';
    desc.textContent =
      'Canvas tools extend your agent with custom capabilities. Enable or disable tools to control which ones the agent can use.';
    group.appendChild(desc);

    // Header row: count + New button
    const headerRow = document.createElement('div');
    headerRow.style.display = 'flex';
    headerRow.style.alignItems = 'center';
    headerRow.style.marginBottom = '8px';

    const countEl = document.createElement('div');
    countEl.className = 'canvas-tools-count';
    countEl.id = 'canvas-tools-count';
    headerRow.appendChild(countEl);

    const newBtn = document.createElement('button');
    newBtn.type = 'button';
    newBtn.className = 'canvas-tools-new-button';
    newBtn.textContent = '+ New Canvas Tool';
    newBtn.addEventListener('click', () => this._openCanvasToolEditor({ mode: 'new' }));
    headerRow.appendChild(newBtn);

    group.appendChild(headerRow);

    const list = document.createElement('div');
    list.className = 'canvas-tools-list';
    list.id = 'canvas-tools-list';

    const loading = document.createElement('div');
    loading.className = 'canvas-tools-loading';
    loading.id = 'canvas-tools-loading';
    loading.textContent = 'Loading tools...';
    list.appendChild(loading);

    const emptyState = document.createElement('div');
    emptyState.className = 'canvas-tools-empty';
    emptyState.id = 'canvas-tools-empty';
    emptyState.style.display = 'none';
    const emptyTitle = document.createElement('p');
    emptyTitle.className = 'canvas-tools-empty-title';
    emptyTitle.textContent = 'No canvas tools available';
    const emptyHint = document.createElement('p');
    emptyHint.className = 'canvas-tools-empty-hint';
    emptyHint.textContent =
      'Canvas tools can be registered by WASM modules, MCP servers, or the canvas SDK.';
    emptyState.appendChild(emptyTitle);
    emptyState.appendChild(emptyHint);
    list.appendChild(emptyState);

    group.appendChild(list);
    section.appendChild(group);

    return section;
  },

  _createCanvasToolItem(tool, index) {
    const item = document.createElement('div');
    item.className = 'canvas-tool-item';
    item.dataset.toolIndex = index;

    // Left: info
    const info = document.createElement('div');
    info.className = 'canvas-tool-info';

    const nameRow = document.createElement('div');
    nameRow.className = 'canvas-tool-name-row';

    const name = document.createElement('span');
    name.className = 'canvas-tool-name';
    name.textContent = tool.name || 'Unnamed tool';
    nameRow.appendChild(name);

    // Badge: prefer `overridden` when is_override is true
    const originSource = tool.origin_source || tool.source || 'unknown';
    const badgeKey = tool.is_override ? 'overridden' : originSource.toLowerCase();
    const badgeText = tool.is_override ? 'overridden' : originSource;
    const sourceBadge = document.createElement('span');
    sourceBadge.className = `canvas-tool-source ${badgeKey}`;
    sourceBadge.textContent = badgeText;
    nameRow.appendChild(sourceBadge);

    info.appendChild(nameRow);

    if (tool.description) {
      const desc = document.createElement('div');
      desc.className = 'canvas-tool-desc';
      desc.textContent = tool.description;
      info.appendChild(desc);
    }

    const meta = document.createElement('div');
    meta.className = 'canvas-tool-meta';
    const parts = [];
    if (tool.kind) parts.push(tool.kind);
    if (tool.args_mode) parts.push(`args: ${tool.args_mode}`);
    if (parts.length) {
      meta.textContent = parts.join(' \u2022 ');
      info.appendChild(meta);
    }

    item.appendChild(info);

    // Right: action buttons + enable toggle
    const actions = document.createElement('div');
    actions.className = 'canvas-tool-actions';

    const src = (tool.origin_source || tool.source || '').toLowerCase();
    const isOverride = !!tool.is_override;

    if (src === 'user') {
      const editBtn = document.createElement('button');
      editBtn.type = 'button';
      editBtn.className = 'canvas-tool-row-action';
      editBtn.textContent = 'Edit';
      editBtn.addEventListener('click', () =>
        this._openCanvasToolEditor({ mode: 'edit', name: tool.name }),
      );
      actions.appendChild(editBtn);

      const delBtn = document.createElement('button');
      delBtn.type = 'button';
      delBtn.className = 'canvas-tool-row-action';
      delBtn.textContent = isOverride ? 'Revert' : 'Delete';
      delBtn.addEventListener('click', () => this._confirmDeleteCanvasTool(tool));
      actions.appendChild(delBtn);
    } else if (src === 'builtin') {
      const forkBtn = document.createElement('button');
      forkBtn.type = 'button';
      forkBtn.className = 'canvas-tool-row-action';
      forkBtn.textContent = 'Fork to edit';
      forkBtn.addEventListener('click', () =>
        this._openCanvasToolEditor({ mode: 'fork', name: tool.name }),
      );
      actions.appendChild(forkBtn);
    }
    // Session tools: no buttons.

    // Enable/disable toggle (unchanged)
    const toggle = document.createElement('label');
    toggle.className = 'mcp-toggle';
    const toggleInput = document.createElement('input');
    toggleInput.type = 'checkbox';
    toggleInput.checked = tool.enabled !== false;
    toggleInput.addEventListener('change', async () => {
      const enabled = toggleInput.checked;
      try {
        await this._sendCanvasToolToggle(tool.name, enabled);
        this._canvasTools[index].enabled = enabled;
      } catch (e) {
        console.error('Failed to toggle canvas tool:', e);
        toggleInput.checked = !enabled;
      }
    });
    const slider = document.createElement('span');
    slider.className = 'mcp-toggle-slider';
    toggle.appendChild(toggleInput);
    toggle.appendChild(slider);
    actions.appendChild(toggle);

    item.appendChild(actions);
    return item;
  },

  async _populateCanvasTools() {
    const list = document.getElementById('canvas-tools-list');
    const loading = document.getElementById('canvas-tools-loading');
    const emptyState = document.getElementById('canvas-tools-empty');
    const countEl = document.getElementById('canvas-tools-count');
    if (!list) return;

    // Show loading
    if (loading) loading.style.display = '';
    if (emptyState) emptyState.style.display = 'none';
    if (countEl) countEl.textContent = '';

    try {
      const result = await NevofluxPage.sendQuery('bridge:request', {
        type: 'canvas.tool.list',
        payload: { include_disabled: true },
      });

      if (loading) loading.style.display = 'none';

      if (!result.success) {
        throw new Error(result.error?.message || 'Failed to fetch tools');
      }

      // sendQuery('bridge:request', ...) returns { success, data }, where
      // `data` is the response from background.js bridgeRespond:
      // { success: true, tools: [...] }. Unwrap both layers.
      const inner = result.data || result;
      const tools = inner.tools || [];
      this._canvasTools = tools;
      this._refreshCanvasToolsList();
    } catch (e) {
      console.warn('Failed to load canvas tools:', e);
      if (loading) loading.style.display = 'none';
      this._canvasTools = [];

      // Show error with retry
      const errorEl = document.createElement('div');
      errorEl.className = 'canvas-tools-error';
      errorEl.textContent = 'Could not load tools. The agent may not be running.';
      const retryBtn = document.createElement('button');
      retryBtn.type = 'button';
      retryBtn.textContent = 'Retry';
      retryBtn.addEventListener('click', () => {
        errorEl.remove();
        this._populateCanvasTools();
      });
      errorEl.appendChild(document.createElement('br'));
      errorEl.appendChild(retryBtn);

      // Remove any existing items first
      for (const child of [...list.children]) {
        if (
          child.id !== 'canvas-tools-loading' &&
          child.id !== 'canvas-tools-empty'
        ) {
          child.remove();
        }
      }
      list.appendChild(errorEl);
    }
  },

  _refreshCanvasToolsList() {
    const list = document.getElementById('canvas-tools-list');
    const emptyState = document.getElementById('canvas-tools-empty');
    const countEl = document.getElementById('canvas-tools-count');
    if (!list) return;

    // Remove existing tool items and error elements
    for (const child of [...list.children]) {
      if (
        child.id !== 'canvas-tools-loading' &&
        child.id !== 'canvas-tools-empty'
      ) {
        child.remove();
      }
    }

    const tools = this._canvasTools || [];

    if (!tools.length) {
      if (emptyState) emptyState.style.display = '';
      if (countEl) countEl.textContent = '';
      return;
    }

    if (emptyState) emptyState.style.display = 'none';

    const enabledCount = tools.filter((t) => t.enabled !== false).length;
    if (countEl) {
      countEl.textContent = `${enabledCount} of ${tools.length} tool${tools.length !== 1 ? 's' : ''} enabled`;
    }

    for (let i = 0; i < tools.length; i++) {
      list.appendChild(this._createCanvasToolItem(tools[i], i));
    }
  },

  async _sendCanvasToolToggle(toolName, enabled) {
    const result = await NevofluxPage.sendQuery('bridge:request', {
      type: 'agent:command',
      payload: {
        command: 'canvas.tool.toggle',
        params: { tool_name: toolName, enabled },
      },
    });
    if (!result.success) {
      throw new Error(result.error?.message || 'Toggle failed');
    }
    const agentResponse = result.data;
    if (agentResponse && !agentResponse.success) {
      throw new Error(agentResponse.error?.message || 'Agent toggle failed');
    }
    // Update the count display
    const countEl = document.getElementById('canvas-tools-count');
    if (countEl && this._canvasTools.length) {
      const enabledCount = this._canvasTools.filter((t) => t.enabled !== false).length;
      countEl.textContent = `${enabledCount} of ${this._canvasTools.length} tool${this._canvasTools.length !== 1 ? 's' : ''} enabled`;
    }
  },

  _canvasToolEditorState: {
    mode: null,           // 'new' | 'edit' | 'fork'
    originalName: null,   // for edit mode — used as expected_name
    editor: null,         // handle from _mountCanvasToolEditor
    saving: false,
  },

  _mountCanvasToolEditor(initialText) {
    const container = document.getElementById('canvas-tool-editor-cm');
    container.innerHTML = '';

    // Plain textarea for v1. CodeMirror integration can be added later.
    const ta = document.createElement('textarea');
    ta.value = initialText || '';
    ta.spellcheck = false;
    ta.style.cssText =
      'width:100%;height:100%;min-height:380px;resize:none;border:0;outline:0;background:transparent;color:inherit;font:inherit;padding:12px 16px;box-sizing:border-box;';
    container.appendChild(ta);

    return {
      getText: () => ta.value,
      setText: (t) => {
        ta.value = t;
      },
      focus: () => ta.focus(),
    };
  },

  async _openCanvasToolEditor({ mode, name }) {
    const modal = document.getElementById('canvas-tool-editor');
    const title = document.getElementById('canvas-tool-editor-title');
    const errorEl = document.getElementById('canvas-tool-editor-error');
    errorEl.hidden = true;
    errorEl.textContent = '';

    let initial = NEW_TOOL_TEMPLATE;
    if (mode === 'edit' || mode === 'fork') {
      try {
        const res = await NevofluxPage.sendQuery('bridge:request', {
          type: 'canvas.tool.get_raw',
          payload: { name },
        });
        const inner = res?.data || res;
        if (!inner?.success) {
          const code = inner?.error?.code;
          if (code === 'not_found') {
            await this._populateCanvasTools();
            return;
          }
          throw new Error(inner?.error?.message || 'Failed to load tool');
        }
        initial = inner.toml_text || '';
      } catch (e) {
        console.error('get_raw failed:', e);
        alert('Could not load the tool for editing.');
        return;
      }
    }

    title.textContent =
      mode === 'new'
        ? 'New Canvas Tool'
        : mode === 'edit'
        ? `Edit: ${name}`
        : `Fork from builtin: ${name}`;

    this._canvasToolEditorState = {
      mode,
      originalName: mode === 'edit' ? name : null,
      editor: this._mountCanvasToolEditor(initial),
      saving: false,
    };

    modal.hidden = false;
    this._canvasToolEditorState.editor.focus();
    this._bindCanvasToolEditorHandlers();
  },

  _closeCanvasToolEditor() {
    const modal = document.getElementById('canvas-tool-editor');
    modal.hidden = true;
    this._canvasToolEditorState = { mode: null, originalName: null, editor: null, saving: false };
  },

  _bindCanvasToolEditorHandlers() {
    const modal = document.getElementById('canvas-tool-editor');
    if (modal._canvasToolBound) return;
    modal._canvasToolBound = true;

    modal.addEventListener('click', (ev) => {
      const action = ev.target?.dataset?.action;
      if (action === 'cancel') this._closeCanvasToolEditor();
      if (action === 'save') this._saveCanvasTool();
    });

    document.addEventListener('keydown', (ev) => {
      if (modal.hidden) return;
      if (ev.key === 'Escape') {
        ev.preventDefault();
        this._closeCanvasToolEditor();
      } else if ((ev.ctrlKey || ev.metaKey) && ev.key === 's') {
        ev.preventDefault();
        this._saveCanvasTool();
      }
    });
  },

  _showCanvasToolEditorError(errorPayload) {
    const errorEl = document.getElementById('canvas-tool-editor-error');
    const code = errorPayload?.code || 'unknown';
    const message = errorPayload?.message || 'Save failed';
    errorEl.hidden = false;

    if (code === 'name_conflict') {
      errorEl.innerHTML = '';
      const label = document.createElement('span');
      label.textContent = `${message}. `;
      errorEl.appendChild(label);
      const match = /'([^']+)'/.exec(message);
      const conflictName = match ? match[1] : null;
      if (conflictName) {
        const a = document.createElement('a');
        a.textContent = 'Edit it instead';
        a.addEventListener('click', (ev) => {
          ev.preventDefault();
          this._closeCanvasToolEditor();
          setTimeout(() => this._openCanvasToolEditor({ mode: 'edit', name: conflictName }), 0);
        });
        errorEl.appendChild(a);
      }
      return;
    }

    const label =
      code === 'toml_parse'
        ? `TOML syntax error: ${message}`
        : code === 'validation'
        ? `Invalid tool: ${message}`
        : code === 'name_changed'
        ? 'Renaming is not supported. Delete this tool and create a new one instead.'
        : code === 'io'
        ? `Could not save: ${message}`
        : message;
    errorEl.textContent = label;
  },

  async _saveCanvasTool() {
    const state = this._canvasToolEditorState;
    if (!state.editor || state.saving) return;

    const saveBtn = document.querySelector(
      '#canvas-tool-editor [data-action="save"]',
    );
    state.saving = true;
    if (saveBtn) {
      saveBtn.disabled = true;
      saveBtn.textContent = 'Saving\u2026';
    }

    try {
      const toml_text = state.editor.getText();
      const payload = { toml_text };
      if (state.mode === 'edit' && state.originalName) {
        payload.expected_name = state.originalName;
      }
      const res = await NevofluxPage.sendQuery('bridge:request', {
        type: 'canvas.tool.save',
        payload,
      });
      const inner = res?.data || res;
      if (!inner?.success) {
        this._showCanvasToolEditorError(inner?.error || { code: 'unknown', message: 'Save failed' });
        return;
      }
      this._closeCanvasToolEditor();
      await this._populateCanvasTools();
    } catch (e) {
      console.error('Save canvas tool failed:', e);
      this._showCanvasToolEditorError({ code: 'io', message: e.message || String(e) });
    } finally {
      state.saving = false;
      if (saveBtn) {
        saveBtn.disabled = false;
        saveBtn.textContent = 'Save';
      }
    }
  },

  _confirmDeleteCanvasTool(tool) {
    const isOverride = !!tool.is_override;
    const msg = isOverride
      ? `Revert '${tool.name}' to its built-in definition? Your customizations will be lost.`
      : `Delete canvas tool '${tool.name}'? The .toml file will be removed.`;
    if (!window.confirm(msg)) return;
    this._deleteCanvasTool(tool.name);
  },

  async _deleteCanvasTool(name) {
    try {
      const res = await NevofluxPage.sendQuery('bridge:request', {
        type: 'canvas.tool.delete',
        payload: { name },
      });
      const inner = res?.data || res;
      if (!inner?.success) {
        alert(`Delete failed: ${inner?.error?.message || 'unknown error'}`);
        return;
      }
      await this._populateCanvasTools();
    } catch (e) {
      console.error('Delete canvas tool failed:', e);
      alert(`Delete failed: ${e.message || e}`);
    }
  },

  // ── My Canvas Section ────────────────────────────────────

  _renderMyCanvasSection() {
    const section = this._createSection('my-canvas', 'My Canvas');
    const group = this._createGroup('My Canvas');

    const desc = document.createElement('p');
    desc.className = 'section-desc';
    desc.textContent =
      'Canvas artifacts you have saved from chat sessions or imported via shared links.';
    group.appendChild(desc);

    // Toolbar: search + filters
    const toolbar = document.createElement('div');
    toolbar.className = 'my-canvas-toolbar';

    const search = document.createElement('input');
    search.type = 'search';
    search.id = 'my-canvas-search';
    search.placeholder = 'Search';
    toolbar.appendChild(search);

    const typeFilter = document.createElement('select');
    typeFilter.id = 'my-canvas-type-filter';
    [
      ['', 'All types'],
      ['text/html', 'HTML'],
      ['text/markdown', 'Markdown'],
      ['image/svg+xml', 'SVG'],
      ['application/json', 'JSON'],
      ['project', 'Project'],
    ].forEach(([val, label]) => {
      const opt = document.createElement('option');
      opt.value = val;
      opt.textContent = label;
      typeFilter.appendChild(opt);
    });
    toolbar.appendChild(typeFilter);

    const sourceFilter = document.createElement('select');
    sourceFilter.id = 'my-canvas-source-filter';
    [
      ['', 'All sources'],
      ['created', 'From sessions'],
      ['imported', 'Imported'],
    ].forEach(([val, label]) => {
      const opt = document.createElement('option');
      opt.value = val;
      opt.textContent = label;
      sourceFilter.appendChild(opt);
    });
    toolbar.appendChild(sourceFilter);

    group.appendChild(toolbar);

    // List container
    const list = document.createElement('div');
    list.id = 'my-canvas-list';
    list.className = 'my-canvas-list';
    group.appendChild(list);

    // Empty state
    const empty = document.createElement('div');
    empty.id = 'my-canvas-empty';
    empty.className = 'my-canvas-empty';
    empty.hidden = true;
    empty.textContent =
      "You haven't saved any Canvas yet. Click the pin on a Canvas card in chat to save it here, or open a shared Canvas link to import.";
    group.appendChild(empty);

    section.appendChild(group);
    return section;
  },

  async _loadMyCanvas() {
    const search = document.getElementById('my-canvas-search')?.value || undefined;
    const typeFilter = document.getElementById('my-canvas-type-filter')?.value || undefined;
    const sourceFilter = document.getElementById('my-canvas-source-filter')?.value || undefined;

    try {
      const resp = await NevofluxPage.sendQuery('bridge:request', {
        type: 'canvas.persist.list',
        payload: {
          search: search || undefined,
          type_filter: typeFilter || undefined,
          source_filter: sourceFilter || undefined,
          sort: 'updated_at',
          limit: 100,
        },
      });

      const data = this._unwrapMyCanvasResponse(resp);
      const items = data.items || [];
      this._renderMyCanvasList(items);
    } catch (e) {
      console.warn('Failed to load My Canvas:', e);
      this._renderMyCanvasList([]);
    }
  },

  _unwrapMyCanvasResponse(resp) {
    // Mirror the unwrap pattern from canvas.tool bridge calls (commit 94187b3ae):
    // sendQuery returns { success, data } where data is the native response.
    if (resp && resp.success && resp.data) return resp.data;
    if (resp && typeof resp === 'object' && 'items' in resp) return resp;
    return resp || {};
  },

  _renderMyCanvasList(items) {
    const list = document.getElementById('my-canvas-list');
    const empty = document.getElementById('my-canvas-empty');
    if (!list || !empty) return;
    list.innerHTML = '';
    if (!items.length) {
      empty.hidden = false;
      return;
    }
    empty.hidden = true;
    for (const item of items) {
      list.appendChild(this._renderMyCanvasRow(item));
    }
  },

  _renderMyCanvasRow(item) {
    const row = document.createElement('div');
    row.className = 'my-canvas-row';
    row.dataset.canvasId = item.id;

    const when = new Date(
      (item.updated_at || item.persisted_at || 0) * 1000
    ).toLocaleString();
    const sourceText =
      item.source?.kind === 'imported'
        ? `Imported (${item.source.share_id})`
        : 'From session';

    const body = document.createElement('div');
    body.className = 'my-canvas-row-body';

    const title = document.createElement('div');
    title.className = 'my-canvas-row-title';
    title.textContent = item.title || 'Untitled Canvas';
    body.appendChild(title);

    const meta = document.createElement('div');
    meta.className = 'my-canvas-row-meta';

    const typeSpan = document.createElement('span');
    typeSpan.className = 'mc-type';
    typeSpan.textContent = item.content_type || '';
    meta.appendChild(typeSpan);

    meta.appendChild(document.createTextNode(' \u00b7 '));

    const updatedSpan = document.createElement('span');
    updatedSpan.className = 'mc-updated';
    updatedSpan.textContent = `edited ${when}`;
    meta.appendChild(updatedSpan);

    meta.appendChild(document.createTextNode(' \u00b7 '));

    const sourceSpan = document.createElement('span');
    sourceSpan.className = 'mc-source';
    sourceSpan.textContent = sourceText;
    meta.appendChild(sourceSpan);

    body.appendChild(meta);
    row.appendChild(body);

    const actions = document.createElement('div');
    actions.className = 'my-canvas-row-actions';

    const openBtn = document.createElement('button');
    openBtn.type = 'button';
    openBtn.className = 'mc-open';
    openBtn.textContent = 'Open';
    openBtn.addEventListener('click', () => this._openMyCanvasTab(item.id));
    actions.appendChild(openBtn);

    const renameBtn = document.createElement('button');
    renameBtn.type = 'button';
    renameBtn.className = 'mc-rename';
    renameBtn.textContent = 'Rename';
    renameBtn.addEventListener('click', () => {
      this._beginRename(row, item);
    });
    actions.appendChild(renameBtn);

    const deleteBtn = document.createElement('button');
    deleteBtn.type = 'button';
    deleteBtn.className = 'mc-delete mc-delete--danger';
    deleteBtn.textContent = 'Delete';
    deleteBtn.addEventListener('click', () => {
      this._confirmDelete(item);
    });
    actions.appendChild(deleteBtn);

    row.appendChild(actions);
    return row;
  },

  _openMyCanvasTab(canvasId) {
    window.open(`nevoflux://canvas/${canvasId}`, '_blank');
  },

  _beginRename(row, item) {
    const titleEl = row.querySelector('.my-canvas-row-title');
    if (!titleEl) return;
    const current = item.title || '';

    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'mc-rename-input';
    input.value = current;
    titleEl.replaceWith(input);
    input.focus();
    input.select();

    let committed = false;
    const commit = async () => {
      if (committed) return;
      committed = true;
      const next = input.value.trim();
      if (!next || next === current) {
        // Unchanged or empty — just refresh to restore the span.
        this._loadMyCanvas();
        return;
      }
      const resp = await NevofluxPage.sendQuery('bridge:request', {
        type: 'canvas.persist.rename',
        payload: { canvas_id: item.id, new_title: next },
      });
      const data = this._unwrapMyCanvasResponse(resp);
      if (!data || data.success === false) {
        const msg = data && data.error && data.error.message ? data.error.message : 'Rename failed';
        window.alert(msg);
      }
      this._loadMyCanvas();
    };
    const cancel = () => {
      if (committed) return;
      committed = true;
      this._loadMyCanvas();
    };

    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); commit(); }
      else if (e.key === 'Escape') { e.preventDefault(); cancel(); }
    });
    input.addEventListener('blur', commit);
  },

  async _confirmDelete(item) {
    const title = item.title || 'Untitled';
    const ok = window.confirm(
      `Delete "${title}" from My Canvas?\n\nThis cannot be undone.`
    );
    if (!ok) return;

    const resp = await NevofluxPage.sendQuery('bridge:request', {
      type: 'canvas.persist.delete',
      payload: { canvas_id: item.id },
    });
    const data = this._unwrapMyCanvasResponse(resp);
    if (!data || data.success === false) {
      const msg = data && data.error && data.error.message ? data.error.message : 'Delete failed';
      window.alert(msg);
      return;
    }
    this._loadMyCanvas();
  },

  _bindMyCanvasControls() {
    let debounceTimer;
    const debouncedLoad = () => {
      clearTimeout(debounceTimer);
      debounceTimer = setTimeout(() => this._loadMyCanvas(), 250);
    };
    for (const id of ['my-canvas-search', 'my-canvas-type-filter', 'my-canvas-source-filter']) {
      const el = document.getElementById(id);
      if (!el) continue;
      el.addEventListener('input', debouncedLoad);
      el.addEventListener('change', debouncedLoad);
    }
  },

  _renderPlaceholderSection(id, title, message) {
    const section = this._createSection(id, title);
    const group = this._createGroup(title);
    const p = document.createElement('p');
    p.className = 'section-placeholder';
    p.textContent = message;
    group.appendChild(p);
    section.appendChild(group);
    return section;
  },

  _renderShortcutsSection() {
    const section = this._createSection('shortcuts', 'Shortcuts');
    const group = this._createGroup('Keyboard Shortcuts');

    const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;

    const row = document.createElement('div');
    row.className = 'shortcut-row';

    const label = document.createElement('span');
    label.className = 'shortcut-label';
    label.textContent = 'Show / Hide Agentic AI Bot';
    row.appendChild(label);

    const keys = document.createElement('span');
    keys.className = 'shortcut-keys';
    const parts = isMac ? ['\u2318', 'Shift', 'A'] : ['Ctrl', 'Shift', 'A'];
    for (let i = 0; i < parts.length; i++) {
      if (i > 0) {
        const sep = document.createElement('span');
        sep.className = 'shortcut-separator';
        sep.textContent = '+';
        keys.appendChild(sep);
      }
      const kbd = document.createElement('kbd');
      kbd.textContent = parts[i];
      keys.appendChild(kbd);
    }
    row.appendChild(keys);

    group.appendChild(row);
    section.appendChild(group);
    return section;
  },

  // ── Settings Persistence ────────────────────────────────

  async _loadSettings() {
    try {
      console.log('[Settings] Loading settings via contentStore:get');
      const result = await NevofluxPage.sendQuery('contentStore:get', {
        key: 'config:settings',
      });
      console.log('[Settings] Loaded settings:', result ? 'found' : 'empty');
      if (result && result.value) {
        this._settings = result.value;
      }
    } catch (e) {
      console.error('[Settings] Failed to load settings:', e);
    }
    this._migrateAgentExecution();
    this._populateFields();
  },

  // Migrate the "Agent execution" tier. The old dead-stub select stored
  // 'confirm'/'auto', which never took effect. Coerce any legacy/invalid value
  // to the safest tier and persist the correction. Canonical list + rationale:
  // agent-execution-tiers.mjs (node-tested). NOTE: old 'auto' must NOT become
  // 'full-auto' — that would silently grant full permissions.
  _migrateAgentExecution() {
    const AGENT_EXECUTION_TIERS = [
      'read-only',
      'browser-auto',
      'browser-auto-local-read',
      'full-auto',
    ];
    const raw = this._getNestedValue(this._settings, 'general.agentExecution');
    if (raw === undefined) return; // fresh profile → select default handles it
    const tier = AGENT_EXECUTION_TIERS.includes(raw) ? raw : 'read-only';
    if (tier !== raw) {
      this._setNestedValue(this._settings, 'general.agentExecution', tier);
      this._scheduleSave();
    }
  },

  _populateFields() {
    // Text/select/password/checkbox/number inputs
    for (const input of document.querySelectorAll('[data-key]')) {
      const key = input.dataset.key;
      const value = this._getNestedValue(this._settings, key);
      if (value === undefined) continue;

      if (input.dataset.avatarRow) {
        // Avatar row — value is a data URL string
        const preview = input.querySelector('.avatar-preview');
        if (preview && value) {
          this._setAvatarPreview(preview, value);
        }
      } else if (input.tagName === 'SELECT') {
        input.value = value;
      } else if (input.type === 'checkbox') {
        input.checked = !!value;
      } else {
        input.value = value;
      }
    }

    // Populate MCP server cards, LLM providers, canvas tools, and My Canvas
    this._populateMcpServers();
    this._populateLlmProviders();
    this._populateCanvasTools();
    this._bindMyCanvasControls();
    this._loadMyCanvas();
    this._loadMdFiles();
  },

  // ── Markdown File Persistence ─────────────────────────

  async _loadMdFiles() {
    const results = await Promise.allSettled(
      this._mdSections.map((md) =>
        this._sendAgentCommand('config.file.read', { filename: md.filename }).then((data) => ({
          md,
          data,
        }))
      )
    );

    for (const result of results) {
      if (result.status === 'fulfilled') {
        const { md, data } = result.value;
        const textarea = document.getElementById(`md-${md.key}`);
        if (!textarea) continue;

        if (data.exists && data.content) {
          textarea.value = data.content;
          this._mdOriginal[md.key] = data.content;
        } else if (md.defaultContent) {
          textarea.value = md.defaultContent;
          this._mdOriginal[md.key] = '';
        } else {
          textarea.value = '';
          this._mdOriginal[md.key] = '';
        }
      } else {
        // Find the md section from the error - extract from the rejection
        // Since we can't easily map back, handle by checking which textareas are still empty
        console.warn('Failed to load a config file:', result.reason);
      }
    }

    // Fill in defaults for any textareas that failed to load
    for (const md of this._mdSections) {
      if (this._mdOriginal[md.key] !== undefined) continue;
      const textarea = document.getElementById(`md-${md.key}`);
      if (textarea && md.defaultContent) {
        textarea.value = md.defaultContent;
      }
      this._mdOriginal[md.key] = '';
    }

    this._checkMdDirty();
  },

  _checkMdDirty() {
    let dirty = false;
    for (const md of this._mdSections) {
      const textarea = document.getElementById(`md-${md.key}`);
      if (!textarea) continue;
      if (textarea.value !== (this._mdOriginal[md.key] ?? '')) {
        dirty = true;
        break;
      }
    }
    this._mdDirty = dirty;
    const btn = document.getElementById('md-save-btn');
    if (btn) btn.disabled = !dirty;
  },

  async _saveMdFiles() {
    const btn = document.getElementById('md-save-btn');
    const status = document.getElementById('md-save-status');
    if (btn) btn.disabled = true;

    let errors = [];
    for (const md of this._mdSections) {
      const textarea = document.getElementById(`md-${md.key}`);
      if (!textarea) continue;
      const content = textarea.value;
      if (content === (this._mdOriginal[md.key] ?? '')) continue;

      // Only skip if both current and original are empty (nothing to write)
      if (!content && !(this._mdOriginal[md.key] ?? '')) continue;

      try {
        await this._sendAgentCommand('config.file.write', {
          filename: md.filename,
          content,
        });
        this._mdOriginal[md.key] = content;
      } catch (e) {
        errors.push(`${md.filename}: ${e.message}`);
      }
    }

    if (errors.length) {
      if (status) {
        status.textContent = `Error: ${errors.join(', ')}`;
        status.className = 'save-status visible error';
        setTimeout(() => {
          status.className = 'save-status';
        }, 4000);
      }
    } else {
      if (status) {
        status.textContent = 'Saved';
        status.className = 'save-status visible';
        setTimeout(() => {
          status.className = 'save-status';
        }, 1500);
      }
    }
    this._checkMdDirty();
  },

  async _populateMcpServers() {
    try {
      const data = await this._sendMcpCommand('mcp.list');
      this._mcpServers = data?.servers || [];
    } catch (e) {
      console.warn('Failed to load MCP servers from agent:', e);
      this._mcpServers = [];
    }
    this._refreshMcpServerList();
  },

  _onFieldChange(key, value) {
    this._setNestedValue(this._settings, key, value);
    this._scheduleSave();
  },

  _scheduleSave() {
    if (this._saveTimer) clearTimeout(this._saveTimer);
    this._saveTimer = setTimeout(() => this._save(), 500);
  },

  async _save() {
    try {
      console.log('[Settings] Saving settings via contentStore:set');
      await NevofluxPage.sendQuery('contentStore:set', {
        key: 'config:settings',
        value: this._settings,
      });
      console.log('[Settings] Settings saved successfully');
      this._showSaveIndicator();
    } catch (e) {
      console.error('[Settings] Failed to save settings:', e);
    }
  },

  _showSaveIndicator() {
    const indicator = document.getElementById('save-indicator');
    if (indicator) {
      indicator.classList.add('visible');
      setTimeout(() => indicator.classList.remove('visible'), 1500);
    }
  },

  _getNestedValue(obj, path) {
    return path.split('.').reduce((o, k) => o?.[k], obj);
  },

  _setNestedValue(obj, path, value) {
    const keys = path.split('.');
    let current = obj;
    for (let i = 0; i < keys.length - 1; i++) {
      if (!current[keys[i]] || typeof current[keys[i]] !== 'object') {
        current[keys[i]] = {};
      }
      current = current[keys[i]];
    }
    current[keys[keys.length - 1]] = value;
  },

  // ── Knowledge Base Section (M4-1) ───────────────────────
  //
  // Shows install-state of the gbrain-backed knowledge base.
  // Status is driven by daemon RPC `kb.wizard.status`.
  // The "Enable Knowledge Base" button is a stub for M4-3, which
  // will wire it to the install wizard modal.

  _renderKnowledgeBaseSection() {
    const section = this._createSection('knowledge-base', 'Knowledge Base');

    const group = this._createGroup('Knowledge Base');

    // Description
    const desc = document.createElement('p');
    desc.className = 'section-desc';
    desc.textContent =
      'Save web pages, conversations, and ideas to a long-term, ' +
      'searchable knowledge base powered by gbrain. Local-first; ' +
      'your data stays on your machine.';
    group.appendChild(desc);

    // Status row
    const statusRow = document.createElement('div');
    statusRow.className = 'kb-status-row';

    const statusLabel = document.createElement('span');
    statusLabel.className = 'kb-status-label';
    statusLabel.textContent = 'Status:';
    statusRow.appendChild(statusLabel);

    const badge = document.createElement('span');
    badge.className = 'kb-status-badge';
    badge.dataset.state = 'unknown';

    const dot = document.createElement('span');
    dot.className = 'kb-status-dot';
    badge.appendChild(dot);

    const text = document.createElement('span');
    text.className = 'kb-status-text';
    text.textContent = 'Checking…';
    badge.appendChild(text);

    statusRow.appendChild(badge);
    group.appendChild(statusRow);

    // Runtime (server) status row — live SupervisorState, distinct from
    // the install/setup badge above.
    const runtimeRow = document.createElement('div');
    runtimeRow.className = 'kb-status-row kb-runtime-row';

    const runtimeLabel = document.createElement('span');
    runtimeLabel.className = 'kb-status-label';
    runtimeLabel.textContent = 'Server:';
    runtimeRow.appendChild(runtimeLabel);

    const runtimeBadge = document.createElement('span');
    runtimeBadge.className = 'kb-status-badge kb-runtime-badge';
    runtimeBadge.dataset.state = 'unknown';
    const runtimeDot = document.createElement('span');
    runtimeDot.className = 'kb-status-dot';
    runtimeBadge.appendChild(runtimeDot);
    const runtimeText = document.createElement('span');
    runtimeText.className = 'kb-status-text kb-runtime-text';
    runtimeText.textContent = '—';
    runtimeBadge.appendChild(runtimeText);
    runtimeRow.appendChild(runtimeBadge);
    group.appendChild(runtimeRow);

    // Detail lines (versions + paths)
    const details = document.createElement('div');
    details.className = 'kb-details';
    for (const [key, label] of [
      ['bun-version', 'bun'],
      ['gbrain-version', 'gbrain'],
      ['brain-dir', 'brain dir'],
    ]) {
      const line = document.createElement('div');
      line.className = 'kb-detail-line';
      line.dataset.key = key;
      const k = document.createElement('span');
      k.className = 'kb-detail-key';
      k.textContent = `${label}: `;
      const v = document.createElement('span');
      v.className = 'kb-detail-value';
      v.textContent = '—';
      line.appendChild(k);
      line.appendChild(v);
      details.appendChild(line);
    }
    group.appendChild(details);

    // Action row
    const actions = document.createElement('div');
    actions.className = 'kb-actions';

    const enableBtn = document.createElement('button');
    enableBtn.type = 'button';
    enableBtn.className = 'kb-enable-btn';
    enableBtn.textContent = 'Enable Knowledge Base';
    enableBtn.disabled = true; // until status loads
    enableBtn.addEventListener('click', () => this._onKbEnableClick());
    actions.appendChild(enableBtn);

    const refreshBtn = document.createElement('button');
    refreshBtn.type = 'button';
    refreshBtn.className = 'kb-refresh-btn';
    refreshBtn.textContent = 'Refresh';
    refreshBtn.addEventListener('click', () => this._refreshKbStatus(section));
    actions.appendChild(refreshBtn);

    // Operational controls — shown only when overall === ready
    // (toggled inside _renderKbStatus).
    const restartBtn = document.createElement('button');
    restartBtn.type = 'button';
    restartBtn.className = 'kb-refresh-btn kb-restart-btn';
    restartBtn.textContent = 'Restart server';
    restartBtn.style.display = 'none';
    restartBtn.addEventListener('click', () => this._onKbRestartClick());
    actions.appendChild(restartBtn);

    const updateBtn = document.createElement('button');
    updateBtn.type = 'button';
    updateBtn.className = 'kb-refresh-btn kb-update-btn';
    updateBtn.textContent = 'Update gbrain…';
    updateBtn.style.display = 'none';
    updateBtn.addEventListener('click', () => this._onKbUpdateToggle(section));
    actions.appendChild(updateBtn);

    // Discoverability hook for nevoflux://brain (M4-4b).
    // Hidden until status === ready; toggled inside _renderKbStatus.
    const browseBtn = document.createElement('a');
    browseBtn.className = 'kb-refresh-btn kb-browse-btn';
    browseBtn.href = 'nevoflux://brain';
    browseBtn.textContent = 'Browse Knowledge Base →';
    browseBtn.style.display = 'none';
    browseBtn.style.textDecoration = 'none';
    actions.appendChild(browseBtn);

    group.appendChild(actions);

    // Update gbrain ref input (revealed by "Update gbrain…").
    const updatePanel = document.createElement('div');
    updatePanel.className = 'kb-update-panel';
    updatePanel.style.display = 'none';

    const updateWarn = document.createElement('div');
    updateWarn.className = 'kb-update-warning';
    updateWarn.textContent =
      '⚠ Updating to a non-default version may break compatibility with this ' +
      'browser’s knowledge-base integration. Leave blank to reinstall the ' +
      'pinned version.';
    updatePanel.appendChild(updateWarn);

    const updateField = document.createElement('div');
    updateField.className = 'kb-update-field';
    const updateInput = document.createElement('input');
    updateInput.type = 'text';
    updateInput.className = 'kb-update-input';
    updateInput.placeholder =
      'gbrain ref (e.g. af5ee1e) — blank = reinstall pinned';
    updateField.appendChild(updateInput);
    const updateConfirm = document.createElement('button');
    updateConfirm.type = 'button';
    updateConfirm.className = 'kb-enable-btn kb-update-confirm';
    updateConfirm.textContent = 'Update';
    updateConfirm.addEventListener('click', () =>
      this._onKbUpdateConfirm(section, updateInput.value.trim())
    );
    updateField.appendChild(updateConfirm);
    updatePanel.appendChild(updateField);
    group.appendChild(updatePanel);

    section.appendChild(group);

    // Kick off initial status fetch (fire-and-forget; errors handled inside).
    this._refreshKbStatus(section);

    return section;
  },

  async _refreshKbStatus(section) {
    try {
      // _sendMcpCommand unwraps both envelope layers ({success,data} ->
      // agent { success, data }) and throws on failure, so a thrown error
      // here means "couldn't reach daemon or RPC failed".
      const report = await this._sendMcpCommand('kb.wizard.status', {});
      this._renderKbStatus(section, report);
    } catch (e) {
      this._renderKbError(section, e?.message ? e.message : String(e));
    }
  },

  _renderKbStatus(section, report) {
    const badge = section.querySelector('.kb-status-badge');
    const text = section.querySelector('.kb-status-text');
    const enableBtn = section.querySelector('.kb-enable-btn');
    if (!badge || !text || !enableBtn) return;

    const stateMap = {
      ready: {
        label: 'Ready',
        color: 'green',
        btnText: 'Enabled',
        btnDisabled: true,
      },
      needs_install: {
        label: 'Not installed',
        color: 'grey',
        btnText: 'Enable Knowledge Base',
        btnDisabled: false,
      },
      needs_init: {
        label: 'Setup required',
        color: 'amber',
        btnText: 'Complete setup',
        btnDisabled: false,
      },
      in_progress: {
        label: 'Installing…',
        color: 'blue',
        btnText: 'Installing…',
        btnDisabled: true,
      },
      failed: {
        label: 'Install failed',
        color: 'red',
        btnText: 'Retry install',
        btnDisabled: false,
      },
    };

    const overall = report && report.overall;
    const s = stateMap[overall] || {
      label: 'Unknown',
      color: 'grey',
      btnText: 'Enable Knowledge Base',
      btnDisabled: false,
    };

    badge.dataset.state = overall || 'unknown';
    badge.style.setProperty('--kb-badge-color', s.color);
    text.textContent = s.label;
    enableBtn.textContent = s.btnText;
    enableBtn.disabled = s.btnDisabled;

    // M4-4b: surface the "Browse" link only when KB is ready.
    const browseBtn = section.querySelector('.kb-browse-btn');
    if (browseBtn) {
      browseBtn.style.display = overall === 'ready' ? '' : 'none';
    }

    // Operational controls (restart / update) only make sense when ready.
    const ready = overall === 'ready';
    const restartBtn = section.querySelector('.kb-restart-btn');
    const updateBtn = section.querySelector('.kb-update-btn');
    if (restartBtn) restartBtn.style.display = ready ? '' : 'none';
    if (updateBtn) updateBtn.style.display = ready ? '' : 'none';
    if (!ready) {
      const panel = section.querySelector('.kb-update-panel');
      if (panel) panel.style.display = 'none';
    }

    // Runtime (server) status.
    const runtimeBadge = section.querySelector('.kb-runtime-badge');
    const runtimeText = section.querySelector('.kb-runtime-text');
    if (runtimeBadge && runtimeText) {
      const rt = (report && report.runtime) || { state: 'unknown' };
      const runtimeMap = {
        running: { label: 'Running', color: 'green' },
        starting: { label: 'Starting…', color: 'blue' },
        restarting: { label: 'Restarting…', color: 'amber' },
        failed: { label: 'Failed', color: 'red' },
        shutdown: { label: 'Stopped', color: 'grey' },
        disabled: { label: 'Disabled', color: 'grey' },
      };
      const r = runtimeMap[rt.state] || { label: 'Unknown', color: 'grey' };
      let runtimeLabel = r.label;
      if (rt.state === 'restarting' && typeof rt.restart_attempt === 'number') {
        runtimeLabel = `Restarting… (attempt ${rt.restart_attempt})`;
      } else if (rt.state === 'failed' && rt.failed_reason) {
        runtimeLabel = `Failed: ${rt.failed_reason}`;
      }
      runtimeBadge.dataset.state = rt.state || 'unknown';
      runtimeBadge.style.setProperty('--kb-badge-color', r.color);
      runtimeText.textContent = runtimeLabel;
    }

    this._setKbDetail(
      section,
      'bun-version',
      report.bun_version || (report.bun_installed ? 'installed' : 'not installed')
    );
    this._setKbDetail(
      section,
      'gbrain-version',
      report.gbrain_version ||
        (report.gbrain_installed ? 'installed' : 'not installed')
    );
    this._setKbDetail(section, 'brain-dir', report.brain_dir || '—');
  },

  _setKbDetail(section, key, value) {
    const el = section.querySelector(
      `.kb-detail-line[data-key="${key}"] .kb-detail-value`
    );
    if (el) el.textContent = value;
  },

  _renderKbError(section, msg) {
    const badge = section.querySelector('.kb-status-badge');
    const text = section.querySelector('.kb-status-text');
    const enableBtn = section.querySelector('.kb-enable-btn');
    if (!badge || !text) return;
    badge.dataset.state = 'error';
    badge.style.setProperty('--kb-badge-color', 'red');
    text.textContent = `Status error: ${msg}`;
    if (enableBtn) {
      enableBtn.disabled = false;
      enableBtn.textContent = 'Enable Knowledge Base';
    }
    const browseBtn = section.querySelector('.kb-browse-btn');
    if (browseBtn) browseBtn.style.display = 'none';
    const restartBtn = section.querySelector('.kb-restart-btn');
    if (restartBtn) restartBtn.style.display = 'none';
    const updateBtn = section.querySelector('.kb-update-btn');
    if (updateBtn) updateBtn.style.display = 'none';
    const runtimeText = section.querySelector('.kb-runtime-text');
    const runtimeBadge = section.querySelector('.kb-runtime-badge');
    if (runtimeText && runtimeBadge) {
      runtimeBadge.dataset.state = 'unknown';
      runtimeBadge.style.setProperty('--kb-badge-color', 'grey');
      runtimeText.textContent = '—';
    }
  },

  _onKbEnableClick() {
    // M4-3: open the install wizard modal. The modal drives the daemon
    // RPCs (kb.wizard.*) and subscribes to system:kb-wizard:progress for
    // live frames.
    this._openKbWizardModal();
  },

  // ── KB Install Wizard Modal (M4-3) ──────────────────────
  //
  // Drives the user through:
  //   install_bun -> install_gbrain -> init_brain
  //
  // For each step we fire `kb.wizard.<step>` (which returns immediately
  // with { started: true }), then watch the EventBus topic
  // `system:kb-wizard:progress` for frames { step, status, progress_pct,
  // log }. After all steps `ok` we re-probe `kb.wizard.status` and only
  // declare success when `overall == "ready"`.

  // Section node from M4-1 (`_renderKnowledgeBaseSection`) — used by
  // cleanup to refresh the badge + version lines.
  _kbSection() {
    return document.getElementById('section-knowledge-base');
  },

  _openKbWizardModal() {
    if (this._kbWizardState) {
      // Already open — bring to front, no-op.
      return;
    }
    const modal = this._buildKbWizardModal();
    // Install flow: drive the install step machine, fall back to status
    // polling if the EventBus subscription can't be established.
    this._kbWizardBootstrap(
      modal,
      () => this._kbWizardStart(),
      () => this._kbWizardStartPolling()
    );
  },

  // Shared modal setup: mount it, init wizard state, subscribe to the
  // progress EventBus, then run `runner()` (the install step machine or
  // a one-off op). `onSubscribeFail` runs if the subscription can't be
  // opened (install uses status polling; ops just rely on the step
  // timeout + the post-close status refresh).
  _kbWizardBootstrap(modal, runner, onSubscribeFail) {
    document.body.appendChild(modal);
    // Use the .show class to flip display:none -> flex.
    requestAnimationFrame(() => modal.classList.add('show'));

    this._kbWizardState = {
      modal,
      currentStep: null,
      cancelled: false,
      finished: false,
      // EventBus subscription bookkeeping.
      channelId: null,
      subscriptionId: null,
      messageListener: null,
      // Per-step resolver and watchdog timer.
      stepResolver: null,
      stepTimeout: null,
      logLines: [],
      // The work to (re-)run: install step machine or a one-off op. Retry
      // re-invokes this so a failed op retries the op, not the installer.
      retryFn: runner,
    };

    this._kbWizardSubscribe()
      .catch((e) => {
        console.warn('[kb-wizard] subscribe failed:', e);
        if (typeof onSubscribeFail === 'function') onSubscribeFail();
      })
      .finally(() => {
        // Runner is safe to call even before subscribe completes — the
        // daemon buffers initial progress lines internally.
        runner();
      });
  },

  // Open the wizard modal to run a single operational step (restart or
  // update gbrain) and stream its progress. Reuses the wizard subscribe +
  // progress machinery. Status auto-refreshes when the modal is closed
  // (see _kbWizardClose).
  _openKbOpModal({ title, subtitle, step, label, params }) {
    if (this._kbWizardState) return; // a wizard/op is already open
    const modal = this._buildKbWizardModal({
      title,
      subtitle,
      steps: [[step, label]],
    });
    this._kbWizardBootstrap(modal, () =>
      this._kbWizardRunOp(step, params || {})
    );
  },

  async _kbWizardRunOp(step, params) {
    try {
      await this._kbWizardRunStep(step, params);
      if (this._kbWizardState?.cancelled) return;
      this._kbWizardComplete();
    } catch (e) {
      if (this._kbWizardState?.cancelled) return;
      this._kbWizardFail(e?.message ? e.message : String(e));
    }
  },

  _onKbRestartClick() {
    this._openKbOpModal({
      title: 'Restart gbrain server',
      subtitle: 'Restarting the gbrain server…',
      step: 'restart',
      label: 'Restart gbrain server',
    });
  },

  _onKbUpdateToggle(section) {
    const panel = section.querySelector('.kb-update-panel');
    if (!panel) return;
    panel.style.display = panel.style.display === 'none' ? '' : 'none';
  },

  _onKbUpdateConfirm(section, ref) {
    const panel = section.querySelector('.kb-update-panel');
    if (panel) panel.style.display = 'none';
    const params = ref ? { ref } : {};
    this._openKbOpModal({
      title: 'Update gbrain package',
      subtitle: ref ? `Updating gbrain to ${ref}…` : 'Reinstalling gbrain…',
      step: 'update_gbrain',
      label: 'Update gbrain package',
      params,
    });
  },

  _buildKbWizardModal(opts) {
    const o = opts || {};
    const modal = document.createElement('div');
    modal.className = 'kb-wizard-modal';
    modal.id = 'kb-wizard-modal';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'kb-wizard-title');

    // Don't close on backdrop click while installing — too easy to lose
    // progress. The Cancel button is the explicit affordance.
    modal.addEventListener('click', (e) => {
      if (e.target !== modal) return;
      if (this._kbWizardState?.finished) {
        this._kbWizardClose();
      }
    });

    const content = document.createElement('div');
    content.className = 'kb-wizard-modal-content';

    // Header
    const header = document.createElement('div');
    header.className = 'kb-wizard-header';
    const title = document.createElement('h2');
    title.id = 'kb-wizard-title';
    title.textContent = o.title || 'Set up Knowledge Base';
    const subtitle = document.createElement('p');
    subtitle.className = 'kb-wizard-subtitle';
    subtitle.textContent =
      o.subtitle ||
      'Installing bun runtime + gbrain CLI, then initializing your brain. ' +
        'You can cancel at any time.';
    header.appendChild(title);
    header.appendChild(subtitle);
    content.appendChild(header);

    // Step list
    const steps = document.createElement('ul');
    steps.className = 'kb-wizard-steps';
    const stepDefs = o.steps || [
      ['install_bun', 'Install bun runtime'],
      ['install_gbrain', 'Install gbrain'],
      ['init_brain', 'Initialize your brain'],
    ];
    for (const [key, label] of stepDefs) {
      const li = document.createElement('li');
      li.className = 'kb-wizard-step';
      li.dataset.step = key;
      li.dataset.status = 'pending';

      const icon = document.createElement('span');
      icon.className = 'kb-wizard-step-icon';
      icon.textContent = '○'; // ○ for pending
      li.appendChild(icon);

      const labelEl = document.createElement('span');
      labelEl.className = 'kb-wizard-step-label';
      labelEl.textContent = label;
      li.appendChild(labelEl);

      steps.appendChild(li);
    }
    content.appendChild(steps);

    // Progress bar
    const progressWrap = document.createElement('div');
    progressWrap.className = 'kb-wizard-progress';
    const progressBar = document.createElement('div');
    progressBar.className = 'kb-wizard-progress-bar';
    progressBar.style.width = '0%';
    progressWrap.appendChild(progressBar);
    content.appendChild(progressWrap);

    // Log box
    const log = document.createElement('pre');
    log.className = 'kb-wizard-log';
    log.textContent = '';
    content.appendChild(log);

    // Status text (last status line, separate from log for prominence)
    const statusMsg = document.createElement('div');
    statusMsg.className = 'kb-wizard-status';
    statusMsg.textContent = 'Checking current state…';
    content.appendChild(statusMsg);

    // Actions
    const actions = document.createElement('div');
    actions.className = 'kb-wizard-actions';

    const cancelBtn = document.createElement('button');
    cancelBtn.type = 'button';
    cancelBtn.className = 'mcp-btn-secondary kb-wizard-cancel-btn';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.addEventListener('click', () => this._kbWizardCancel());
    actions.appendChild(cancelBtn);

    const retryBtn = document.createElement('button');
    retryBtn.type = 'button';
    retryBtn.className = 'mcp-btn-primary kb-wizard-retry-btn';
    retryBtn.textContent = 'Retry';
    retryBtn.style.display = 'none';
    retryBtn.addEventListener('click', () => this._kbWizardRetry());
    actions.appendChild(retryBtn);

    const doneBtn = document.createElement('button');
    doneBtn.type = 'button';
    doneBtn.className = 'mcp-btn-primary kb-wizard-done-btn';
    doneBtn.textContent = 'Done';
    doneBtn.style.display = 'none';
    doneBtn.addEventListener('click', () => this._kbWizardClose());
    actions.appendChild(doneBtn);

    content.appendChild(actions);
    modal.appendChild(content);
    return modal;
  },

  async _kbWizardSubscribe() {
    // Path 1: persistent EventBus channel via NevofluxParent's
    // events:channel_open + bridge events.subscribe (mirrors the canvas.js
    // SDK shim, see canvas.js ~line 932). The channel keeps push frames
    // flowing past bridge:request's 5-second push grace window.
    const state = this._kbWizardState;
    if (!state) return;
    const channelId =
      'kbwiz_' + Date.now() + '_' + Math.random().toString(36).slice(2, 8);

    await NevofluxPage.sendQuery('events:channel_open', { channelId });
    state.channelId = channelId;

    // Install the message listener BEFORE issuing subscribe, so we don't
    // miss any early-arriving frames.
    state.messageListener = (event) => {
      const detail = event.detail;
      if (!detail || detail.type !== 'bridge:push') return;
      const msg = detail.msg;
      if (!msg || msg.type !== 'events:delivery') return;
      const ev = msg.payload?.event;
      if (!ev || ev.topic !== 'system:kb-wizard:progress') return;
      // ev.payload is the WizardProgress JSON (snake_case from Rust serde).
      try {
        this._kbWizardOnProgress(ev.payload);
      } catch (err) {
        console.warn('[kb-wizard] progress handler failed:', err);
      }
    };
    window.addEventListener('NevofluxMessage', state.messageListener);

    // Subscribe via the daemon's EventBus. background.js's
    // events.subscribe handler accepts our channel_id and routes
    // matching deliveries back through bridge:push.
    const res = await NevofluxPage.sendQuery('bridge:request', {
      type: 'events.subscribe',
      payload: {
        patterns: ['system:kb-wizard:progress'],
        replay_sticky: false,
        channel_id: channelId,
      },
    });
    if (!res || res.success === false) {
      // Tear down the channel; caller will fall back to polling.
      try {
        await NevofluxPage.sendQuery('events:channel_close', { channelId });
      } catch (_e) {}
      state.channelId = null;
      window.removeEventListener('NevofluxMessage', state.messageListener);
      state.messageListener = null;
      throw new Error(res?.error?.message || 'events.subscribe failed');
    }
    const data = res.data?.data !== undefined ? res.data.data : res.data;
    state.subscriptionId = data?.subscription_id || data?.subscriptionId || null;
  },

  _kbWizardStartPolling() {
    // Polling fallback: re-probe kb.wizard.status every 1s while a step
    // is running. This loses the per-line `log` text from upstream
    // stderr — we only get status transitions (running -> ok / failed).
    const state = this._kbWizardState;
    if (!state) return;
    this._kbWizardAppendLog(
      '[wizard] EventBus subscribe unavailable; using polling. ' +
        'Step transitions will be reported but command output will not stream.'
    );
    state.pollHandle = setInterval(async () => {
      if (!this._kbWizardState || this._kbWizardState.cancelled) return;
      try {
        const status = await this._sendMcpCommand('kb.wizard.status', {});
        const cur = this._kbWizardState.currentStep;
        if (!cur) return;
        // Translate overall + per-component flags into a synthetic frame.
        const stepDone =
          (cur === 'install_bun' && status.bun_installed) ||
          (cur === 'install_gbrain' && status.gbrain_installed) ||
          (cur === 'init_brain' && status.brain_initialized);
        const frameStatus = stepDone
          ? 'ok'
          : status.overall === 'failed'
            ? 'failed'
            : 'running';
        this._kbWizardOnProgress({
          step: cur,
          status: frameStatus,
          progress_pct: stepDone ? 100 : 50,
          log:
            frameStatus === 'ok'
              ? `[poll] ${cur} complete`
              : frameStatus === 'failed'
                ? `[poll] ${cur} failed (overall=${status.overall})`
                : `[poll] ${cur} still running…`,
        });
      } catch (e) {
        console.warn('[kb-wizard] poll failed:', e);
      }
    }, 1000);
  },

  async _kbWizardStart() {
    try {
      const status = await this._sendMcpCommand('kb.wizard.status', {});
      if (this._kbWizardState?.cancelled) return;
      this._kbWizardSetStatus(`Current state: ${status.overall}`);

      if (status.overall === 'ready') {
        this._kbWizardAppendLog('[wizard] Already installed — nothing to do.');
        this._kbWizardComplete();
        return;
      }

      if (!status.bun_installed) {
        await this._kbWizardRunStep('install_bun');
      } else {
        this._kbWizardSetStepDone('install_bun');
      }
      if (this._kbWizardState?.cancelled) return;

      if (!status.gbrain_installed) {
        await this._kbWizardRunStep('install_gbrain');
      } else {
        this._kbWizardSetStepDone('install_gbrain');
      }
      if (this._kbWizardState?.cancelled) return;

      if (!status.brain_initialized) {
        await this._kbWizardRunStep('init_brain');
      } else {
        this._kbWizardSetStepDone('init_brain');
      }
      if (this._kbWizardState?.cancelled) return;

      // Verify
      const finalStatus = await this._sendMcpCommand('kb.wizard.status', {});
      if (finalStatus.overall === 'ready') {
        this._kbWizardComplete();
      } else {
        this._kbWizardFail(
          `Final status was ${finalStatus.overall}, expected ready`
        );
      }
    } catch (e) {
      if (this._kbWizardState?.cancelled) return;
      this._kbWizardFail(e?.message ? e.message : String(e));
    }
  },

  _kbWizardRunStep(step, params = {}) {
    return new Promise((resolve, reject) => {
      if (!this._kbWizardState) {
        reject(new Error('wizard state gone'));
        return;
      }
      if (this._kbWizardState.cancelled) {
        reject(new Error('cancelled by user'));
        return;
      }
      this._kbWizardState.currentStep = step;
      this._kbWizardState.stepResolver = { resolve, reject };
      this._kbWizardSetStepActive(step);
      this._kbWizardSetStatus(`Running: ${step}`);

      // install_bun + init_brain can be slow on cold disks / slow networks.
      // install_gbrain + update_gbrain are dominated by bun's network fetch.
      const TIMEOUT =
        step === 'install_gbrain' || step === 'update_gbrain'
          ? 6 * 60 * 1000
          : 10 * 60 * 1000;
      this._kbWizardState.stepTimeout = setTimeout(() => {
        if (this._kbWizardState?.stepResolver) {
          this._kbWizardState.stepResolver.reject(
            new Error(`step ${step} timed out after ${TIMEOUT / 60000}min`)
          );
          this._kbWizardState.stepResolver = null;
        }
      }, TIMEOUT);

      // Fire the RPC. The response is `{ started: true }`; the actual work
      // streams progress via the EventBus subscription set up earlier.
      this._sendMcpCommand(`kb.wizard.${step}`, params).catch((e) => {
        if (this._kbWizardState?.stepResolver) {
          clearTimeout(this._kbWizardState.stepTimeout);
          this._kbWizardState.stepResolver.reject(e);
          this._kbWizardState.stepResolver = null;
        }
      });
    });
  },

  _kbWizardOnProgress(frame) {
    const state = this._kbWizardState;
    if (!state || state.cancelled) return;
    if (!frame || typeof frame !== 'object') return;

    if (frame.log) this._kbWizardAppendLog(frame.log);
    if (typeof frame.progress_pct === 'number') {
      this._kbWizardSetProgress(frame.progress_pct);
    }

    // Map detect_bun frames into the install_bun step row so the user
    // gets some visual feedback even when bun is already present.
    const frameStep = frame.step === 'detect_bun' ? 'install_bun' : frame.step;

    if (frameStep === state.currentStep) {
      if (frame.status === 'ok') {
        this._kbWizardSetStepDone(frameStep);
        clearTimeout(state.stepTimeout);
        if (state.stepResolver) {
          state.stepResolver.resolve();
          state.stepResolver = null;
        }
      } else if (frame.status === 'failed') {
        this._kbWizardSetStepFailed(frameStep, frame.log);
        clearTimeout(state.stepTimeout);
        if (state.stepResolver) {
          state.stepResolver.reject(
            new Error(`step ${frameStep} failed: ${frame.log || ''}`)
          );
          state.stepResolver = null;
        }
      } else if (frame.status === 'cancelled') {
        state.cancelled = true;
        clearTimeout(state.stepTimeout);
        if (state.stepResolver) {
          state.stepResolver.reject(new Error('cancelled'));
          state.stepResolver = null;
        }
      }
    }
  },

  _kbWizardAppendLog(line) {
    const state = this._kbWizardState;
    if (!state || !line) return;
    state.logLines.push(String(line));
    // Cap at 30 most-recent lines to keep the box bounded.
    if (state.logLines.length > 30) {
      state.logLines.splice(0, state.logLines.length - 30);
    }
    const logEl = state.modal.querySelector('.kb-wizard-log');
    if (logEl) {
      logEl.textContent = state.logLines.join('\n');
      logEl.scrollTop = logEl.scrollHeight;
    }
  },

  _kbWizardSetProgress(pct) {
    const bar = this._kbWizardState?.modal?.querySelector(
      '.kb-wizard-progress-bar'
    );
    if (!bar) return;
    const clamped = Math.max(0, Math.min(100, Number(pct) || 0));
    bar.style.width = `${clamped}%`;
  },

  _kbWizardSetStatus(text) {
    const el = this._kbWizardState?.modal?.querySelector('.kb-wizard-status');
    if (el) el.textContent = text;
  },

  _kbWizardSetStepActive(step) {
    const li = this._kbWizardState?.modal?.querySelector(
      `.kb-wizard-step[data-step="${step}"]`
    );
    if (!li) return;
    li.dataset.status = 'running';
    const icon = li.querySelector('.kb-wizard-step-icon');
    if (icon) icon.textContent = '◒'; // ◒ for in-progress
  },

  _kbWizardSetStepDone(step) {
    const li = this._kbWizardState?.modal?.querySelector(
      `.kb-wizard-step[data-step="${step}"]`
    );
    if (!li) return;
    li.dataset.status = 'ok';
    const icon = li.querySelector('.kb-wizard-step-icon');
    if (icon) icon.textContent = '✓'; // ✓
  },

  _kbWizardSetStepFailed(step, msg) {
    const li = this._kbWizardState?.modal?.querySelector(
      `.kb-wizard-step[data-step="${step}"]`
    );
    if (!li) return;
    li.dataset.status = 'failed';
    const icon = li.querySelector('.kb-wizard-step-icon');
    if (icon) icon.textContent = '✗'; // ✗
    this._kbWizardSetStatus(`Failed at ${step}${msg ? ': ' + msg : ''}`);
  },

  async _kbWizardCancel() {
    const state = this._kbWizardState;
    if (!state) return;
    state.cancelled = true;
    this._kbWizardAppendLog('[wizard] Cancellation requested…');
    try {
      await this._sendMcpCommand('kb.wizard.cancel', {});
    } catch (e) {
      // Non-fatal — the daemon may have already finished or the RPC may
      // simply not be available. We still tear down the UI.
      console.warn('[kb-wizard] cancel rpc failed:', e);
    }
    // Reject any in-flight step so _kbWizardStart bails out.
    if (state.stepResolver) {
      clearTimeout(state.stepTimeout);
      state.stepResolver.reject(new Error('cancelled by user'));
      state.stepResolver = null;
    }
    this._kbWizardSetStatus('Cancelled.');
    this._kbWizardShowDone(/* labelOverride */ 'Close');
    state.finished = true;
  },

  _kbWizardRetry() {
    // Reset visible state and restart the step machine from scratch.
    const state = this._kbWizardState;
    if (!state) return;
    state.cancelled = false;
    state.finished = false;
    state.currentStep = null;
    state.logLines = [];
    for (const li of state.modal.querySelectorAll('.kb-wizard-step')) {
      li.dataset.status = 'pending';
      const icon = li.querySelector('.kb-wizard-step-icon');
      if (icon) icon.textContent = '○';
    }
    const log = state.modal.querySelector('.kb-wizard-log');
    if (log) log.textContent = '';
    this._kbWizardSetProgress(0);
    this._kbWizardSetStatus('Retrying…');
    state.modal.querySelector('.kb-wizard-retry-btn').style.display = 'none';
    state.modal.querySelector('.kb-wizard-done-btn').style.display = 'none';
    state.modal.querySelector('.kb-wizard-cancel-btn').style.display = '';
    // Re-run whatever this modal was driving (install machine or a
    // one-off op), not always the installer.
    if (typeof state.retryFn === 'function') {
      state.retryFn();
    } else {
      this._kbWizardStart();
    }
  },

  _kbWizardComplete() {
    const state = this._kbWizardState;
    if (!state) return;
    state.finished = true;
    this._kbWizardSetProgress(100);
    this._kbWizardSetStatus('Knowledge Base is ready.');
    this._kbWizardAppendLog('[wizard] All steps complete. Ready.');
    this._kbWizardShowDone('Done');
  },

  _kbWizardFail(msg) {
    const state = this._kbWizardState;
    if (!state) return;
    state.finished = true;
    this._kbWizardSetStatus(`Failed: ${msg}`);
    this._kbWizardAppendLog(`[wizard] FAILED: ${msg}`);
    // Show Retry + Close (re-labelled Done).
    state.modal.querySelector('.kb-wizard-retry-btn').style.display = '';
    state.modal.querySelector('.kb-wizard-done-btn').style.display = '';
    state.modal.querySelector('.kb-wizard-done-btn').textContent = 'Close';
    state.modal.querySelector('.kb-wizard-cancel-btn').style.display = 'none';
  },

  _kbWizardShowDone(label) {
    const state = this._kbWizardState;
    if (!state) return;
    const cancel = state.modal.querySelector('.kb-wizard-cancel-btn');
    const done = state.modal.querySelector('.kb-wizard-done-btn');
    if (cancel) cancel.style.display = 'none';
    if (done) {
      done.textContent = label || 'Done';
      done.style.display = '';
    }
  },

  async _kbWizardCleanup() {
    const state = this._kbWizardState;
    if (!state) return;
    if (state.stepTimeout) clearTimeout(state.stepTimeout);
    if (state.pollHandle) clearInterval(state.pollHandle);
    if (state.messageListener) {
      window.removeEventListener('NevofluxMessage', state.messageListener);
      state.messageListener = null;
    }
    // Unsubscribe and close the EventBus channel. Both calls are
    // best-effort — failure here just leaks a server-side handle until
    // the page closes.
    if (state.subscriptionId) {
      try {
        await NevofluxPage.sendQuery('bridge:request', {
          type: 'events.unsubscribe',
          payload: { subscription_id: state.subscriptionId },
        });
      } catch (e) {
        console.warn('[kb-wizard] unsubscribe failed:', e);
      }
    }
    if (state.channelId) {
      try {
        await NevofluxPage.sendQuery('events:channel_close', {
          channelId: state.channelId,
        });
      } catch (e) {
        console.warn('[kb-wizard] channel_close failed:', e);
      }
    }
  },

  async _kbWizardClose() {
    const state = this._kbWizardState;
    if (!state) return;
    await this._kbWizardCleanup();
    state.modal.classList.remove('show');
    state.modal.remove();
    this._kbWizardState = null;
    // Refresh the KB section so the badge + version lines reflect the
    // post-install state.
    const section = this._kbSection();
    if (section) {
      this._refreshKbStatus(section);
    }
  },

  // ── Packs Section ───────────────────────────────────────
  //
  // Lists installed packs and installs / uninstalls / updates / validates
  // them via the daemon RPCs:
  //   pack.list / pack.status / pack.validate / pack.install /
  //   pack.uninstall / pack.update
  //
  // The UX deliberately mirrors the Knowledge Base section above and
  // reuses its CSS classes (kb-status-*, kb-actions, kb-enable-btn,
  // kb-refresh-btn, kb-wizard-modal, mcp-btn-*) so styling stays
  // consistent without adding new CSS.
  //
  // IMPORTANT (ESM-in-chrome): settings.js is loaded as a classic <script>
  // (see settings.html) and cannot `import` an ES module at runtime. The
  // pure helpers below live in `pack-ui-logic.mjs` as the *tested source
  // of truth* (tests/unit/pack-ui-logic.test.mjs) and are inline-duplicated
  // here under `_PackLogic`. Keep the two in sync.

  _PackLogic: {
    // Mirror of pack-ui-logic.mjs `isRemoteSource`.
    isRemoteSource(source) {
      if (typeof source !== 'string') return false;
      const s = source.trim();
      return s.startsWith('github:') || s.startsWith('https://github.com/');
    },

    // Mirror of pack-ui-logic.mjs `packListToRows`.
    packListToRows(report) {
      let packs;
      if (Array.isArray(report)) {
        packs = report;
      } else if (report && Array.isArray(report.packs)) {
        packs = report.packs;
      } else {
        packs = [];
      }
      return packs
        .filter((p) => p && typeof p === 'object')
        .map((p) => ({
          name: typeof p.name === 'string' ? p.name : '',
          version: p.version != null ? String(p.version) : '',
          installedAt:
            p.installed_at != null
              ? String(p.installed_at)
              : p.installedAt != null
                ? String(p.installedAt)
                : '',
        }));
    },

    // Mirror of pack-ui-logic.mjs `validateResultMessage`.
    validateResultMessage(result) {
      if (result && result.ok) {
        return 'Validation passed — no policy violations.';
      }
      const raw = (result && result.violations) || [];
      const violations = Array.isArray(raw) ? raw : [];
      const messages = violations
        .map((v) => {
          if (v == null) return '';
          if (typeof v === 'string') return v;
          if (typeof v === 'object') {
            return String(v.message || v.rule || v.detail || JSON.stringify(v));
          }
          return String(v);
        })
        .filter((m) => m.length > 0);
      if (messages.length === 0) {
        return 'Validation failed.';
      }
      const count = messages.length;
      const noun = count === 1 ? 'violation' : 'violations';
      return (
        `Validation failed (${count} ${noun}):\n` +
        messages.map((m) => `• ${m}`).join('\n')
      );
    },

    // Mirror of pack-ui-logic.mjs `installParams`.
    installParams(source, opts = {}) {
      const src = typeof source === 'string' ? source.trim() : '';
      const params = {
        ...(this.isRemoteSource(src)
          ? { source: src }
          : { manifest_path: src }),
        wait: opts.wait === false ? false : true,
      };
      if (opts.force) params.force = true;
      return params;
    },

    // Mirror of pack-ui-logic.mjs `summarizePackProgress`.
    summarizePackProgress(frame, opId) {
      const f = frame || {};
      const status = typeof f.status === 'string' ? f.status : '';
      const pct = typeof f.progress_pct === 'number' ? f.progress_pct : 0;
      const phase = typeof f.phase === 'string' ? f.phase : '';
      const log = typeof f.log === 'string' ? f.log : '';
      const ok = status === 'Ok';
      const failed =
        status === 'Failed' || status === 'RolledBack' || status === 'Cancelled';
      return {
        matched: f.op_id === opId,
        pct,
        phase,
        status,
        line: `[${phase} ${pct}%]${log ? ` ${log}` : ''}`,
        terminal: ok || failed,
        ok,
        failed,
      };
    },

    // Mirror of pack-ui-logic.mjs `inspectParams`.
    inspectParams(source) {
      return { source: typeof source === 'string' ? source.trim() : '' };
    },

    // Mirror of pack-ui-logic.mjs `uninstallParams`.
    uninstallParams(name, opts = {}) {
      const params = {
        name: typeof name === 'string' ? name : '',
        purge_data: opts.purgeData === true,
      };
      if (opts.force) params.force = true;
      return params;
    },

    // Mirror of pack-ui-logic.mjs `updateParams`.
    updateParams(source) {
      const src = typeof source === 'string' ? source.trim() : '';
      return this.isRemoteSource(src)
        ? { source: src }
        : { manifest_path: src };
    },

    // Mirror of pack-ui-logic.mjs `summarizeInspect`.
    summarizeInspect(data) {
      const d = data && typeof data === 'object' ? data : {};
      const pack = d.pack && typeof d.pack === 'object' ? d.pack : {};
      const comps =
        d.components && typeof d.components === 'object' ? d.components : {};

      const lines = [];

      const name =
        typeof pack.name === 'string' && pack.name ? pack.name : '(unnamed pack)';
      const version =
        pack.version != null && String(pack.version) ? String(pack.version) : '';
      const description =
        typeof pack.description === 'string' && pack.description
          ? pack.description
          : '';
      let header = name;
      if (version) header += ` ${version}`;
      if (description) header += ` — ${description}`;
      lines.push(header);

      const asArray = (v) => (Array.isArray(v) ? v : []);

      const skills = asArray(comps.skills).filter(
        (s) => typeof s === 'string' && s
      );
      if (skills.length) {
        lines.push(`Skills: ${skills.join(', ')}`);
      }

      const canvasTools = asArray(comps.canvas_tools);
      if (canvasTools.length) {
        const rendered = canvasTools
          .map((t) => {
            if (t && typeof t === 'object') {
              const tn = typeof t.name === 'string' ? t.name : '';
              const bin = typeof t.binary === 'string' ? t.binary : '';
              if (tn && bin) return `${tn} (runs: ${bin})`;
              if (tn) return tn;
              if (bin) return `(runs: ${bin})`;
              return '';
            }
            return typeof t === 'string' ? t : '';
          })
          .filter((s) => s.length > 0);
        if (rendered.length) {
          lines.push(`Canvas tools: ${rendered.join(', ')}`);
          lines.push(
            '  ⚠ Canvas tools run the binaries listed above on your machine.'
          );
        }
      }

      const seed = asArray(comps.seed).filter(
        (s) => typeof s === 'string' && s
      );
      if (seed.length) {
        lines.push(`Seed pages: ${seed.join(', ')}`);
      }

      if (typeof comps.dashboard === 'string' && comps.dashboard) {
        lines.push(`Dashboard: ${comps.dashboard}`);
      }

      if (comps.knowledge) {
        lines.push('Knowledge: yes');
      }

      const rawViolations = Array.isArray(d.violations) ? d.violations : [];
      const violations = rawViolations
        .map((v) => {
          if (v == null) return '';
          if (typeof v === 'string') return v;
          if (typeof v === 'object') {
            return String(v.message || v.rule || v.detail || JSON.stringify(v));
          }
          return String(v);
        })
        .filter((m) => m.length > 0);

      if (violations.length === 0) {
        lines.push('Violations: <none>');
      } else {
        const count = violations.length;
        const noun = count === 1 ? 'violation' : 'violations';
        lines.push(`⚠ Violations (${count} ${noun}):`);
        for (const v of violations) {
          lines.push(`  • ${v}`);
        }
      }

      return {
        text: lines.join('\n'),
        violations,
        hasViolations: violations.length > 0,
      };
    },

    // Mirror of pack-ui-logic.mjs `packErrorMessage`.
    packErrorMessage(error) {
      if (error == null) return 'Unknown error.';
      if (typeof error === 'string') return error;
      const code = error.code || (error.error && error.error.code);
      const message =
        error.message || (error.error && error.error.message) || '';
      switch (code) {
        case 'KNOWLEDGE_UNSUPPORTED':
          return (
            'Packs are not supported in this build or configuration. ' +
            (message
              ? `(${message})`
              : 'Knowledge-base support is unavailable.')
          );
        case 'INSTALL_FAILED':
          return `Install failed${message ? `: ${message}` : '.'}`;
        case 'UPDATE_FAILED':
          return `Update failed${message ? `: ${message}` : '.'}`;
        case 'UNINSTALL_FAILED':
          return `Uninstall failed${message ? `: ${message}` : '.'}`;
        case 'VALIDATION_FAILED':
          return `Validation failed${message ? `: ${message}` : '.'}`;
        default:
          break;
      }
      if (message) return message;
      if (code) return String(code);
      return 'Unknown error.';
    },
  },

  _packsSection() {
    return document.getElementById('section-packs');
  },

  _renderPacksSection() {
    const section = this._createSection('packs', 'Packs');
    const group = this._createGroup('Packs');

    // Description
    const desc = document.createElement('p');
    desc.className = 'section-desc';
    desc.textContent =
      'Install, update, and remove knowledge packs. Packs bundle tools, ' +
      'skills, and data that extend what NevoFlux can do. Manifests are ' +
      'read from disk by the daemon.';
    group.appendChild(desc);

    // Status row (mirrors the KB status badge).
    const statusRow = document.createElement('div');
    statusRow.className = 'kb-status-row';
    const statusLabel = document.createElement('span');
    statusLabel.className = 'kb-status-label';
    statusLabel.textContent = 'Status:';
    statusRow.appendChild(statusLabel);

    const badge = document.createElement('span');
    badge.className = 'kb-status-badge pack-status-badge';
    badge.dataset.state = 'unknown';
    const dot = document.createElement('span');
    dot.className = 'kb-status-dot';
    badge.appendChild(dot);
    const text = document.createElement('span');
    text.className = 'kb-status-text pack-status-text';
    text.textContent = 'Checking…';
    badge.appendChild(text);
    statusRow.appendChild(badge);
    group.appendChild(statusRow);

    // Installed-packs list container (rows injected by _renderPacksList).
    const list = document.createElement('div');
    list.className = 'kb-details pack-list';
    group.appendChild(list);

    // Action row.
    const actions = document.createElement('div');
    actions.className = 'kb-actions';

    const installBtn = document.createElement('button');
    installBtn.type = 'button';
    installBtn.className = 'kb-enable-btn pack-install-btn';
    installBtn.textContent = 'Install Pack…';
    installBtn.addEventListener('click', () => this._onPackInstallClick());
    actions.appendChild(installBtn);

    const refreshBtn = document.createElement('button');
    refreshBtn.type = 'button';
    refreshBtn.className = 'kb-refresh-btn pack-refresh-btn';
    refreshBtn.textContent = 'Refresh';
    refreshBtn.addEventListener('click', () => this._refreshPacksStatus(section));
    actions.appendChild(refreshBtn);

    group.appendChild(actions);
    section.appendChild(group);

    // Kick off initial fetch (fire-and-forget; errors handled inside).
    this._refreshPacksStatus(section);

    return section;
  },

  async _refreshPacksStatus(section) {
    try {
      const report = await this._sendMcpCommand('pack.list', {});
      this._renderPacksList(section, report);
    } catch (e) {
      this._renderPacksError(section, e);
    }
  },

  _renderPacksList(section, report) {
    const badge = section.querySelector('.pack-status-badge');
    const text = section.querySelector('.pack-status-text');
    const list = section.querySelector('.pack-list');
    if (!badge || !text || !list) return;

    const rows = this._PackLogic.packListToRows(report);
    list.innerHTML = '';

    if (rows.length === 0) {
      badge.dataset.state = 'needs_install';
      badge.style.setProperty('--kb-badge-color', 'grey');
      text.textContent = 'No packs installed';

      const empty = document.createElement('div');
      empty.className = 'kb-detail-line pack-empty';
      empty.textContent = 'No packs installed yet. Use “Install Pack…”.';
      list.appendChild(empty);
      return;
    }

    badge.dataset.state = 'ready';
    badge.style.setProperty('--kb-badge-color', 'green');
    const noun = rows.length === 1 ? 'pack' : 'packs';
    text.textContent = `${rows.length} ${noun} installed`;

    for (const row of rows) {
      list.appendChild(this._buildPackRow(section, row));
    }
  },

  _buildPackRow(section, row) {
    const line = document.createElement('div');
    line.className = 'kb-detail-line pack-row';
    line.dataset.pack = row.name;

    const info = document.createElement('span');
    info.className = 'kb-detail-key pack-row-info';
    let label = row.name;
    if (row.version) label += `  v${row.version}`;
    if (row.installedAt) label += `  (installed ${row.installedAt})`;
    info.textContent = label;
    line.appendChild(info);

    const rowActions = document.createElement('span');
    rowActions.className = 'pack-row-actions';

    const updateBtn = document.createElement('button');
    updateBtn.type = 'button';
    updateBtn.className = 'kb-refresh-btn pack-row-update-btn';
    updateBtn.textContent = 'Update';
    updateBtn.addEventListener('click', () =>
      this._onPackUpdateClick(section, row.name)
    );
    rowActions.appendChild(updateBtn);

    const uninstallBtn = document.createElement('button');
    uninstallBtn.type = 'button';
    uninstallBtn.className = 'kb-refresh-btn pack-row-uninstall-btn';
    uninstallBtn.textContent = 'Uninstall';
    uninstallBtn.addEventListener('click', () =>
      this._onPackUninstallClick(section, row.name)
    );
    rowActions.appendChild(uninstallBtn);

    line.appendChild(rowActions);
    return line;
  },

  _renderPacksError(section, error) {
    const badge = section.querySelector('.pack-status-badge');
    const text = section.querySelector('.pack-status-text');
    const list = section.querySelector('.pack-list');
    if (!badge || !text) return;
    const msg = this._PackLogic.packErrorMessage(error);
    badge.dataset.state = 'error';
    badge.style.setProperty('--kb-badge-color', 'red');
    text.textContent = `Status error: ${msg}`;
    if (list) {
      list.innerHTML = '';
      const errLine = document.createElement('div');
      errLine.className = 'kb-detail-line pack-error';
      errLine.textContent = msg;
      list.appendChild(errLine);
    }
  },

  // ── Pack install flow (modal, mirrors the KB wizard chrome) ─────────

  _onPackInstallClick() {
    if (this._packModalState) return; // a pack modal is already open
    this._openPackInstallModal();
  },

  _openPackInstallModal() {
    const modal = this._buildPackInstallModal();
    document.body.appendChild(modal);
    requestAnimationFrame(() => modal.classList.add('show'));
    this._packModalState = { modal, busy: false, pendingRemoteSource: null };
    this._packResetConfirm();

    const input = modal.querySelector('.pack-manifest-input');
    if (input) input.focus();
  },

  _buildPackInstallModal() {
    const modal = document.createElement('div');
    modal.className = 'kb-wizard-modal pack-install-modal';
    modal.id = 'pack-install-modal';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'pack-install-title');

    modal.addEventListener('click', (e) => {
      if (e.target !== modal) return;
      if (!this._packModalState?.busy) this._closePackModal();
    });

    const content = document.createElement('div');
    content.className = 'kb-wizard-modal-content';

    const header = document.createElement('div');
    header.className = 'kb-wizard-header';
    const title = document.createElement('h2');
    title.id = 'pack-install-title';
    title.textContent = 'Install Pack';
    const subtitle = document.createElement('p');
    subtitle.className = 'kb-wizard-subtitle';
    subtitle.textContent =
      'Enter a local manifest path or a GitHub source ' +
      '(github:user/repo[/sub][@ref]). Local manifests can be validated ' +
      'first; GitHub sources are inspected, previewed, and confirmed before ' +
      'install.';
    header.appendChild(title);
    header.appendChild(subtitle);
    content.appendChild(header);

    // Source input: local manifest path or a GitHub source.
    const field = document.createElement('div');
    field.className = 'kb-update-field pack-manifest-field';
    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'kb-update-input pack-manifest-input';
    input.placeholder =
      '/path/to/pack/manifest.toml  or  github:user/repo[/sub][@ref]';
    // Re-evaluate the action buttons whenever the source changes so a
    // pending remote-install confirmation doesn't apply to a new source.
    input.addEventListener('input', () => this._onPackSourceInput());
    field.appendChild(input);
    content.appendChild(field);

    // Result / log box (reuses wizard log styling).
    const log = document.createElement('pre');
    log.className = 'kb-wizard-log pack-install-log';
    log.textContent = '';
    content.appendChild(log);

    // Unreviewed-source warning (hidden until a remote source is inspected).
    const warning = document.createElement('div');
    warning.className = 'kb-wizard-status pack-install-warning';
    warning.style.display = 'none';
    warning.textContent =
      '⚠ Unreviewed source — installing trusts the author; canvas-tools ' +
      'can run commands on your machine.';
    content.appendChild(warning);

    const statusMsg = document.createElement('div');
    statusMsg.className = 'kb-wizard-status pack-install-status';
    statusMsg.textContent = 'Enter a manifest path or GitHub source to begin.';
    content.appendChild(statusMsg);

    // Actions.
    const actions = document.createElement('div');
    actions.className = 'kb-wizard-actions';

    const cancelBtn = document.createElement('button');
    cancelBtn.type = 'button';
    cancelBtn.className = 'mcp-btn-secondary pack-install-cancel-btn';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.addEventListener('click', () => this._closePackModal());
    actions.appendChild(cancelBtn);

    const validateBtn = document.createElement('button');
    validateBtn.type = 'button';
    validateBtn.className = 'mcp-btn-secondary pack-validate-btn';
    validateBtn.textContent = 'Validate';
    validateBtn.addEventListener('click', () => this._onPackValidate());
    actions.appendChild(validateBtn);

    const installBtn = document.createElement('button');
    installBtn.type = 'button';
    installBtn.className = 'mcp-btn-primary pack-install-confirm-btn';
    installBtn.textContent = 'Install';
    installBtn.addEventListener('click', () => this._onPackInstall());
    actions.appendChild(installBtn);

    content.appendChild(actions);
    modal.appendChild(content);
    return modal;
  },

  _packModalEl(selector) {
    return this._packModalState?.modal?.querySelector(selector) || null;
  },

  _packModalLog(line) {
    const el = this._packModalEl('.pack-install-log');
    if (!el || !line) return;
    el.textContent += (el.textContent ? '\n' : '') + String(line);
    el.scrollTop = el.scrollHeight;
  },

  _packModalStatus(text) {
    const el = this._packModalEl('.pack-install-status');
    if (el) el.textContent = text;
  },

  _packModalSetBusy(busy) {
    if (!this._packModalState) return;
    this._packModalState.busy = busy;
    for (const sel of [
      '.pack-validate-btn',
      '.pack-install-confirm-btn',
      '.pack-manifest-input',
    ]) {
      const el = this._packModalEl(sel);
      if (el) el.disabled = busy;
    }
  },

  _packManifestPath() {
    const input = this._packModalEl('.pack-manifest-input');
    return input ? input.value.trim() : '';
  },

  // Show/hide the "unreviewed source" warning banner.
  _packModalShowWarning(show) {
    const el = this._packModalEl('.pack-install-warning');
    if (el) el.style.display = show ? '' : 'none';
  },

  // Reset the remote-install confirmation handshake (used when the source
  // changes, the modal opens, or an install finishes).
  _packResetConfirm() {
    if (this._packModalState) {
      this._packModalState.pendingRemoteSource = null;
    }
    const installBtn = this._packModalEl('.pack-install-confirm-btn');
    if (installBtn) installBtn.textContent = 'Install';
    this._packModalShowWarning(false);
  },

  // Called on every keystroke in the source input. If the user edits the
  // source after a remote inspect, any pending confirmation is voided so a
  // second click can't install a source the preview no longer matches.
  _onPackSourceInput() {
    if (!this._packModalState) return;
    if (this._packModalState.busy) return;
    const source = this._packManifestPath();
    const pending = this._packModalState.pendingRemoteSource;
    if (pending != null && pending !== source) {
      this._packResetConfirm();
    }
  },

  async _onPackValidate() {
    if (!this._packModalState || this._packModalState.busy) return;
    const source = this._packManifestPath();
    if (!source) {
      this._packModalStatus('Please enter a manifest path or GitHub source first.');
      return;
    }
    this._packModalSetBusy(true);
    this._packModalStatus('Validating…');
    try {
      // Remote sources validate by `source`; local paths by `manifest_path`.
      const params = this._PackLogic.isRemoteSource(source)
        ? { source }
        : { manifest_path: source };
      const result = await this._sendMcpCommand('pack.validate', params);
      const msg = this._PackLogic.validateResultMessage(result);
      this._packModalLog(msg);
      this._packModalStatus(result && result.ok ? 'Validation passed.' : 'Validation failed.');
    } catch (e) {
      const msg = this._PackLogic.packErrorMessage(e);
      this._packModalLog(`Validation error: ${msg}`);
      this._packModalStatus(`Error: ${msg}`);
    } finally {
      this._packModalSetBusy(false);
    }
  },

  async _onPackInstall() {
    if (!this._packModalState || this._packModalState.busy) return;
    const source = this._packManifestPath();
    if (!source) {
      this._packModalStatus('Please enter a manifest path or GitHub source first.');
      return;
    }

    // Remote (GitHub) sources go through inspect → preview → confirm.
    if (this._PackLogic.isRemoteSource(source)) {
      const pending = this._packModalState.pendingRemoteSource;
      if (pending === source) {
        // Second click: the user has reviewed the preview — install.
        await this._doPackInstall(source, { remote: true });
      } else {
        // First click: inspect and render the preview, then arm confirm.
        await this._onPackInspect(source);
      }
      return;
    }

    // Local manifest path: keep the existing validate → install behaviour.
    await this._doPackInstall(source, { remote: false });
  },

  // Inspect a remote source and render a preview + unreviewed-source
  // warning, then arm the explicit confirm step (unless violations block).
  async _onPackInspect(source) {
    this._packModalSetBusy(true);
    this._packModalStatus('Inspecting source…');
    this._packModalLog(`Inspecting ${source}…`);
    try {
      const result = await this._sendMcpCommand(
        'pack.inspect',
        this._PackLogic.inspectParams(source)
      );
      const summary = this._PackLogic.summarizeInspect(result);
      this._packModalLog(summary.text);

      if (summary.hasViolations) {
        // Block install when the daemon reports policy violations.
        this._packResetConfirm();
        this._packModalStatus(
          'Install blocked — this source has policy violations.'
        );
        return;
      }

      // Arm the explicit confirm handshake.
      this._packModalState.pendingRemoteSource = source;
      this._packModalShowWarning(true);
      const installBtn = this._packModalEl('.pack-install-confirm-btn');
      if (installBtn) installBtn.textContent = 'Confirm install (unreviewed)';
      this._packModalStatus(
        'Review the preview above, then confirm to install.'
      );
    } catch (e) {
      const msg = this._PackLogic.packErrorMessage(e);
      this._packModalLog(`Inspect failed: ${msg}`);
      this._packModalStatus(`Error: ${msg}`);
      this._packResetConfirm();
    } finally {
      this._packModalSetBusy(false);
    }
  },

  // Perform the actual install. For local sources runs a non-blocking
  // pre-flight validate; remote sources were already inspected/confirmed.
  async _doPackInstall(source, opts = {}) {
    this._packModalSetBusy(true);
    this._packModalStatus('Installing…');
    this._packModalLog(`Installing from ${source}…`);
    try {
      if (!opts.remote) {
        // Optional pre-flight validation for local manifests: warn but
        // don't block (the daemon validates again during install).
        try {
          const vr = await this._sendMcpCommand('pack.validate', {
            manifest_path: source,
          });
          if (vr && vr.ok === false) {
            this._packModalLog(this._PackLogic.validateResultMessage(vr));
          }
        } catch (_ve) {
          // Non-fatal — proceed to install and let it surface real errors.
        }
      }

      // Prefer the streaming (wait:false) path so a large pack (hundreds of
      // seed pages → hundreds of sequential gbrain round-trips) isn't killed by
      // the 30s bridge request timeout. Falls back to a synchronous install if
      // the progress channel can't be established.
      let result = null;
      try {
        await this._packInstallStreaming(source);
      } catch (streamErr) {
        if (streamErr && streamErr.__packInstallFailed) {
          throw streamErr; // a real install failure — surface it below
        }
        if (streamErr && streamErr.__packInstallIndeterminate) {
          // Stream lost but the install is likely still completing in the
          // daemon — don't claim success/failure or hang the modal.
          this._packModalLog(
            'Progress stream lost — the install may still be completing in the ' +
              'background. Check the Packs list in a moment.'
          );
          this._packModalStatus('Install running in background…');
          const sec = this._packsSection();
          if (sec) this._refreshPacksStatus(sec);
          return; // leave the modal open; finally clears the busy spinner
        }
        // Progress channel unavailable — degrade to the synchronous install
        // (works for small packs; large ones may still time out, as before).
        this._packModalLog('Live progress unavailable — installing synchronously…');
        const params = this._PackLogic.installParams(source, {});
        result = await this._sendMcpCommand('pack.install', params);
      }
      const ver = result && result.version ? ` (v${result.version})` : '';
      const files =
        result && Array.isArray(result.files)
          ? ` — ${result.files.length} file(s)`
          : '';
      this._packModalLog(`Installed${ver}${files}.`);
      this._packModalStatus('Pack installed.');
      this._packResetConfirm();
      // Refresh the section list behind the modal.
      const section = this._packsSection();
      if (section) this._refreshPacksStatus(section);
      // Auto-close shortly after success. Capture the current modal state so a
      // stale timer can't close a different modal the user reopened meanwhile.
      const st = this._packModalState;
      this._packCloseTimer = setTimeout(() => {
        if (this._packModalState === st && !st.busy) {
          this._closePackModal();
        }
      }, 900);
    } catch (e) {
      const msg = this._PackLogic.packErrorMessage(e);
      this._packModalLog(`Install failed: ${msg}`);
      this._packModalStatus(`Error: ${msg}`);
    } finally {
      this._packModalSetBusy(false);
    }
  },

  // Install a pack via the daemon's async `wait:false` path and stream
  // `system:pack:progress` frames into the modal log. Resolves on the terminal
  // Ok frame; rejects with `__packInstallFailed` on a terminal failure. Throws a
  // plain Error (no `__packInstallFailed`) if the progress channel can't be set
  // up or the daemon doesn't return an op_id, so the caller can fall back to a
  // synchronous install. Mirrors the KB-wizard EventBus subscribe (see
  // _kbWizardSubscribe).
  async _packInstallStreaming(source) {
    const channelId =
      'packinst_' + Date.now() + '_' + Math.random().toString(36).slice(2, 8);
    let messageListener = null;
    let subscriptionId = null;
    let opId = null;
    let watchdogTimer = null;
    let lastFrameAt = Date.now();
    let settled = false;

    // Baseline for the poll fallback: packs already installed before we start.
    let baselineNames = null;
    try {
      const lst = await this._sendMcpCommand('pack.list', {});
      baselineNames = new Set(
        (lst?.packs || []).map((p) => p && p.name).filter(Boolean)
      );
    } catch (_e) {}

    const teardown = async () => {
      if (watchdogTimer) {
        clearInterval(watchdogTimer);
        watchdogTimer = null;
      }
      if (messageListener) {
        window.removeEventListener('NevofluxMessage', messageListener);
        messageListener = null;
      }
      if (subscriptionId) {
        try {
          await NevofluxPage.sendQuery('bridge:request', {
            type: 'events.unsubscribe',
            payload: { subscription_id: subscriptionId },
          });
        } catch (_e) {}
      }
      try {
        await NevofluxPage.sendQuery('events:channel_close', { channelId });
      } catch (_e) {}
    };

    // 1) Open a persistent push channel (survives bridge:request's 5s grace).
    await NevofluxPage.sendQuery('events:channel_open', { channelId });

    // 2) Wire terminal completion to incoming frames BEFORE subscribing so no
    //    early frame is missed.
    const completion = new Promise((resolve, reject) => {
      const settle = (kind, val) => {
        if (settled) return;
        settled = true;
        if (kind === 'reject') reject(val);
        else resolve(val);
      };
      messageListener = (event) => {
        const detail = event.detail;
        if (!detail || detail.type !== 'bridge:push') return;
        const msg = detail.msg;
        if (!msg || msg.type !== 'events:delivery') return;
        const ev = msg.payload?.event;
        if (!ev || ev.topic !== 'system:pack:progress') return;
        lastFrameAt = Date.now();
        // Installs are serialized (one modal at a time), so the first frame's
        // op_id is ours — latch it to avoid dropping frames that arrive before
        // the pack.install reply returns the op_id.
        if (!opId && ev.payload && ev.payload.op_id) opId = ev.payload.op_id;
        const view = this._PackLogic.summarizePackProgress(ev.payload, opId);
        if (!view.matched) return;
        if (view.line) this._packModalLog(view.line);
        this._packModalStatus(`Installing… ${view.pct}%`);
        if (view.terminal) {
          if (view.ok) {
            settle('resolve');
          } else {
            const err = new Error(view.log || `Install ${view.status}`);
            err.__packInstallFailed = true;
            settle('reject', err);
          }
        }
      };
      window.addEventListener('NevofluxMessage', messageListener);

      // Watchdog: if the progress stream goes silent (e.g. the event-bus
      // subscription timed out during a long phase), poll pack.list so the modal
      // can never hang. Resolve when a new pack appears; after a hard cap, reject
      // as indeterminate so the caller closes the modal with a background note
      // rather than spinning forever.
      const SILENCE_MS = 90000; // begin polling after 90s with no frames
      const GIVE_UP_MS = 600000; // stop waiting after 10 min of silence
      watchdogTimer = setInterval(async () => {
        if (settled) return;
        const silent = Date.now() - lastFrameAt;
        if (silent < SILENCE_MS) return;
        try {
          const lst = await this._sendMcpCommand('pack.list', {});
          const names = new Set(
            (lst?.packs || []).map((p) => p && p.name).filter(Boolean)
          );
          if (baselineNames && [...names].some((n) => !baselineNames.has(n))) {
            this._packModalLog('Install completed (progress stream ended early).');
            settle('resolve');
            return;
          }
        } catch (_e) {}
        if (silent > GIVE_UP_MS) {
          const err = new Error('progress stream lost; install may still be running');
          err.__packInstallIndeterminate = true;
          settle('reject', err);
        }
      }, 5000);
    });

    // 3) Subscribe to the topic over our channel.
    const sub = await NevofluxPage.sendQuery('bridge:request', {
      type: 'events.subscribe',
      payload: {
        patterns: ['system:pack:progress'],
        replay_sticky: false,
        channel_id: channelId,
      },
    });
    if (!sub || sub.success === false) {
      await teardown();
      throw new Error(sub?.error?.message || 'events.subscribe failed');
    }
    const subData = sub.data?.data !== undefined ? sub.data.data : sub.data;
    subscriptionId = subData?.subscription_id || subData?.subscriptionId || null;

    // 4) Fire the install (wait:false) and capture the op_id.
    try {
      const params = this._PackLogic.installParams(source, { wait: false });
      const started = await this._sendMcpCommand('pack.install', params);
      opId = started?.op_id || started?.opId || opId;
      if (!opId) {
        throw new Error('daemon did not return an op_id for wait:false install');
      }
    } catch (e) {
      await teardown();
      throw e; // setup-time failure → caller falls back to synchronous install
    }

    // 5) Await the terminal frame, then tear down.
    try {
      await completion;
    } finally {
      await teardown();
    }
  },

  _closePackModal() {
    const state = this._packModalState;
    if (!state) return;
    if (this._packCloseTimer) {
      clearTimeout(this._packCloseTimer);
      this._packCloseTimer = null;
    }
    state.modal.classList.remove('show');
    state.modal.remove();
    this._packModalState = null;
  },

  // ── Per-row update / uninstall ──────────────────────────────────────

  async _onPackUpdateClick(section, name) {
    // Update reads a fresh manifest: prompt for a local path or a GitHub
    // source (github:user/repo[/sub][@ref]).
    const manifestPath = this._promptText(
      `Update “${name}” — local manifest path or GitHub source ` +
        `(github:user/repo[/sub][@ref]):`,
      ''
    );
    if (manifestPath == null) return; // cancelled
    const trimmed = manifestPath.trim();
    if (!trimmed) return;
    try {
      const params = this._PackLogic.updateParams(trimmed);
      await this._sendMcpCommand('pack.update', params);
      this._refreshPacksStatus(section);
    } catch (e) {
      this._renderPacksError(section, e);
    }
  },

  async _onPackUninstallClick(section, name) {
    const choice = this._confirmPackUninstall(name);
    if (!choice.confirmed) return;
    try {
      const params = this._PackLogic.uninstallParams(name, {
        purgeData: choice.purgeData,
      });
      await this._sendMcpCommand('pack.uninstall', params);
      this._refreshPacksStatus(section);
    } catch (e) {
      this._renderPacksError(section, e);
    }
  },

  // Confirmation with an optional "also delete this pack's data" choice.
  // Uses a two-step native prompt so we don't need to hand-roll a modal
  // for a destructive-but-rare action; defaults purge OFF.
  _confirmPackUninstall(name) {
    const ok = this._confirm(
      `Uninstall pack “${name}”?\n\n` +
        'Click OK to continue, or Cancel to keep it.'
    );
    if (!ok) return { confirmed: false, purgeData: false };
    const purge = this._confirm(
      `Also delete “${name}”'s data from disk (purge_data)?\n\n` +
        'OK = delete data too (irreversible). Cancel = keep data.'
    );
    return { confirmed: true, purgeData: purge };
  },

  // Thin wrappers around window globals so tests / non-browser callers
  // could stub them; in the chrome page these resolve to the real dialogs.
  _confirm(message) {
    try {
      return window.confirm(message);
    } catch (_e) {
      return false;
    }
  },

  _promptText(message, def) {
    try {
      return window.prompt(message, def);
    } catch (_e) {
      return null;
    }
  },
};

document.addEventListener('DOMContentLoaded', () => Settings.init());
