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
    container.appendChild(this._renderMcpSection());
    container.appendChild(this._renderCanvasToolsSection());
    container.appendChild(this._renderMyCanvasSection());
    container.appendChild(this._renderKnowledgeBaseSection());
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
          ['auto', 'Auto-open'],
          ['manual', 'Manual only'],
        ],
        'auto'
      )
    );
    sidebarGroup.appendChild(
      this._createSelectRow(
        'Agent execution',
        'general.agentExecution',
        [
          ['confirm', 'Confirm before actions'],
          ['auto', 'Auto-execute'],
        ],
        'confirm'
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

  _renderLLMSection() {
    const section = this._createSection('llm', 'AI Models');

    // Guidance banner (hidden by default)
    const banner = document.createElement('div');
    banner.className = 'llm-guidance-banner';
    banner.id = 'llm-guidance-banner';
    banner.style.display = 'none';
    section.appendChild(banner);

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

    return section;
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
    if (!llmGrid) return;
    llmGrid.innerHTML = '';
    if (agentGrid) agentGrid.innerHTML = '';

    const providers = this._llmProviders;
    if (!providers.length) {
      const empty = document.createElement('div');
      empty.className = 'llm-loading';
      empty.textContent = 'No providers available.';
      llmGrid.appendChild(empty);
      return;
    }

    for (const provider of providers) {
      const isAgent = provider.type === 'cli' || provider.type === 'agent';
      const targetGrid = isAgent && agentGrid ? agentGrid : llmGrid;
      targetGrid.appendChild(this._createProviderCard(provider));
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
    saveBtn.addEventListener('click', () => this._saveLlmProvider());
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

    const title = document.getElementById('llm-modal-title');
    title.textContent = `Configure ${provider.display_name || provider.id}`;

    const subtitle = document.getElementById('llm-modal-subtitle');
    const typeLabels = { cli: 'CLI provider', local: 'Local provider', agent: 'Agent provider', service: 'Cloud API provider' };
    subtitle.textContent = typeLabels[provider.type] || 'Cloud API provider';

    const isOpenClaw = provider.id === 'openclaw';

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

  async _sendAgentCommand(command, params = {}) {
    return this._sendMcpCommand(command, params);
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

  async _sendMcpCommand(command, params = {}) {
    // NevofluxParent wraps the result: { success, data: <bridgeRespond payload> }
    const result = await NevofluxPage.sendQuery('bridge:request', {
      type: 'agent:command',
      payload: { command, params },
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
    this._populateFields();
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
    };

    // Kick off subscribe + step machine in parallel; both are async
    // and the step machine waits for resolver invocations triggered
    // by the subscription's progress frames.
    this._kbWizardSubscribe()
      .catch((e) => {
        console.warn('[kb-wizard] subscribe failed, falling back to polling:', e);
        this._kbWizardStartPolling();
      })
      .finally(() => {
        // _kbWizardStart is safe to call even before subscribe completes —
        // the daemon buffers initial progress lines internally.
        this._kbWizardStart();
      });
  },

  _buildKbWizardModal() {
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
    title.textContent = 'Set up Knowledge Base';
    const subtitle = document.createElement('p');
    subtitle.className = 'kb-wizard-subtitle';
    subtitle.textContent =
      'Installing bun runtime + gbrain CLI, then initializing your brain. ' +
      'You can cancel at any time.';
    header.appendChild(title);
    header.appendChild(subtitle);
    content.appendChild(header);

    // Step list
    const steps = document.createElement('ul');
    steps.className = 'kb-wizard-steps';
    for (const [key, label] of [
      ['install_bun', 'Install bun runtime'],
      ['install_gbrain', 'Install gbrain'],
      ['init_brain', 'Initialize your brain'],
    ]) {
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

  _kbWizardRunStep(step) {
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
      // install_gbrain is dominated by bun's network fetch; cap at 5min.
      const TIMEOUT =
        step === 'install_gbrain' ? 5 * 60 * 1000 : 10 * 60 * 1000;
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
      this._sendMcpCommand(`kb.wizard.${step}`, {}).catch((e) => {
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
    this._kbWizardStart();
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
    this._kbWizardSetStatus(`Install failed: ${msg}`);
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
};

document.addEventListener('DOMContentLoaded', () => Settings.init());
