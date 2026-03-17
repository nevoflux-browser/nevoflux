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
    container.appendChild(
      this._renderPlaceholderSection(
        'plugins',
        'Plugins',
        'Plugin management will be available in a future update.'
      )
    );
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

    const providerGroup = this._createGroup('LLM Providers');

    const grid = document.createElement('div');
    grid.className = 'llm-providers-grid';
    grid.id = 'llm-providers-grid';

    // Loading state
    const loading = document.createElement('div');
    loading.className = 'llm-loading';
    loading.id = 'llm-loading';
    loading.textContent = 'Loading providers...';
    grid.appendChild(loading);

    providerGroup.appendChild(grid);
    section.appendChild(providerGroup);

    return section;
  },

  async _populateLlmProviders() {
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
    const grid = document.getElementById('llm-providers-grid');
    if (!grid) return;
    grid.innerHTML = '';

    const providers = this._llmProviders;
    if (!providers.length) {
      const empty = document.createElement('div');
      empty.className = 'llm-loading';
      empty.textContent = 'No providers available.';
      grid.appendChild(empty);
      return;
    }

    for (const provider of providers) {
      grid.appendChild(this._createProviderCard(provider));
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
    subtitle.textContent = `${provider.type === 'cli' ? 'CLI provider' : provider.type === 'local' ? 'Local provider' : 'Cloud API provider'}`;

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

    this._llmModal.classList.add('show');
    setTimeout(() => document.getElementById('llm-modal-apikey').focus(), 50);
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

    const params = { provider: providerId, set_active: setActive };
    if (apiKey) params.api_key = apiKey;
    if (model !== undefined) params.model = model;
    // Always send base_url so it can be cleared (empty string = remove)
    params.base_url = baseUrl;

    saveBtn.disabled = true;
    statusEl.textContent = 'Saving...';
    statusEl.className = 'llm-modal-status';

    try {
      await this._sendAgentCommand('config.llm.set', params);
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

    // Populate MCP server cards and LLM providers
    this._populateMcpServers();
    this._populateLlmProviders();
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
};

document.addEventListener('DOMContentLoaded', () => Settings.init());
