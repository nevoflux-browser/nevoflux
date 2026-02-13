/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

/**
 * Settings page controller.
 *
 * Renders section-based settings UI with form controls. Settings are
 * persisted via ContentStore (config:{key} namespace) through the actor.
 */
const Settings = {
  _currentSection: "general",
  _saveTimer: null,
  _settings: {},

  init() {
    this._currentSection = NevofluxPage.getParam("section", "general");
    this._setupNavigation();
    this._renderSections();
    this._activateSection(this._currentSection);
    this._loadSettings();
  },

  // ── Navigation ──────────────────────────────────────────

  _setupNavigation() {
    const nav = document.getElementById("settings-nav");
    nav.addEventListener("click", (e) => {
      const link = e.target.closest("a[data-section]");
      if (!link) return;
      e.preventDefault();
      const section = link.dataset.section;
      this._activateSection(section);

      // Update URL
      const url = new URL(window.location.href);
      url.searchParams.set("section", section);
      history.replaceState(null, "", url.toString());
    });
  },

  _activateSection(sectionId) {
    this._currentSection = sectionId;

    // Update nav
    for (const link of document.querySelectorAll(".settings-nav a")) {
      link.classList.toggle("active", link.dataset.section === sectionId);
    }

    // Show/hide sections
    for (const section of document.querySelectorAll(".settings-section")) {
      section.classList.toggle("active", section.id === `section-${sectionId}`);
    }
  },

  // ── Section Rendering ───────────────────────────────────

  _renderSections() {
    const container = document.getElementById("settings-content");
    container.innerHTML = "";

    container.appendChild(this._renderGeneralSection());
    container.appendChild(this._renderLLMSection());
    container.appendChild(this._renderMcpSection());
    container.appendChild(this._renderPlaceholderSection("plugins", "Plugins", "Plugin management will be available in a future update."));
    container.appendChild(this._renderPlaceholderSection("shortcuts", "Shortcuts", "Keyboard shortcut customization will be available in a future update."));
  },

  // ── General Section ─────────────────────────────────────

  _renderGeneralSection() {
    const section = this._createSection("general", "General");

    // Sidebar behavior
    const sidebarGroup = this._createGroup("Sidebar & Agent");
    sidebarGroup.appendChild(this._createSelectRow(
      "Sidebar default", "general.sidebarBehavior",
      [["auto", "Auto-open"], ["manual", "Manual only"]],
      "auto"
    ));
    sidebarGroup.appendChild(this._createSelectRow(
      "Agent execution", "general.agentExecution",
      [["confirm", "Confirm before actions"], ["auto", "Auto-execute"]],
      "confirm"
    ));
    sidebarGroup.appendChild(this._createTextRow(
      "Data storage", "general.dataPath", "~/.config/nevoflux/",
      "Path for artifact and session data"
    ));
    section.appendChild(sidebarGroup);

    // Project Context: IDENTITY
    const identityGroup = this._createGroup("Identity");
    identityGroup.appendChild(this._el("h3", "Define your agent's persona"));
    identityGroup.appendChild(this._createAvatarRow("Avatar", "identity.avatar"));
    identityGroup.appendChild(this._createTextRow("Name", "identity.name", "Nevo"));
    identityGroup.appendChild(this._createTextRow("Creature", "identity.creature", ""));
    identityGroup.appendChild(this._createTextRow("Vibe", "identity.vibe", ""));
    identityGroup.appendChild(this._createTextRow("Emoji", "identity.emoji", ""));
    section.appendChild(identityGroup);

    // Project Context: SOUL
    const soulGroup = this._createGroup("Soul");
    soulGroup.appendChild(this._el("h3", "Define your agent's values"));
    soulGroup.appendChild(this._createListRow("Core truths", "soul.coreTruths"));
    soulGroup.appendChild(this._createListRow("Boundaries", "soul.boundaries"));
    soulGroup.appendChild(this._createTextareaRow("Purpose", "soul.purpose", ""));
    section.appendChild(soulGroup);

    // Project Context: USER
    const userGroup = this._createGroup("User Profile");
    userGroup.appendChild(this._el("h3", "Tell your agent about yourself"));
    userGroup.appendChild(this._createTextRow("Name", "user.name", ""));
    userGroup.appendChild(this._createTextRow("Preferred title", "user.preferredTitle", ""));
    userGroup.appendChild(this._createSelectRow(
      "Timezone", "user.timezone",
      this._getTimezones(),
      Intl.DateTimeFormat().resolvedOptions().timeZone
    ));
    userGroup.appendChild(this._createTextareaRow("Notes", "user.notes", "", "Any details your agent should know about you"));
    section.appendChild(userGroup);

    return section;
  },

  // ── LLM Section ─────────────────────────────────────────

  _renderLLMSection() {
    const section = this._createSection("llm", "AI Models");

    const providerGroup = this._createGroup("LLM Provider");
    providerGroup.appendChild(this._createSelectRow(
      "Provider", "llm.provider",
      [
        ["anthropic", "Anthropic"],
        ["openai", "OpenAI"],
        ["google", "Google AI"],
        ["local", "Local Model"],
      ],
      "anthropic"
    ));
    providerGroup.appendChild(this._createPasswordRow("API Key", "llm.apiKey", ""));
    providerGroup.appendChild(this._createSelectRow(
      "Default model", "llm.defaultModel",
      [
        ["claude-sonnet-4-5-20250929", "Claude Sonnet 4.5"],
        ["claude-opus-4-6", "Claude Opus 4.6"],
        ["claude-haiku-4-5-20251001", "Claude Haiku 4.5"],
        ["gpt-4o", "GPT-4o"],
        ["gpt-4o-mini", "GPT-4o Mini"],
        ["gemini-2.0-flash", "Gemini 2.0 Flash"],
      ],
      "claude-sonnet-4-5-20250929"
    ));
    section.appendChild(providerGroup);

    return section;
  },

  // ── MCP Servers Section ────────────────────────────────

  _renderMcpSection() {
    const section = this._createSection("mcp", "MCP Servers");

    // Server list group
    const serverGroup = this._createGroup("Servers");

    const serverList = document.createElement("div");
    serverList.className = "mcp-server-list";
    serverList.id = "mcp-server-list";

    // Empty state shown when no servers
    const emptyState = document.createElement("div");
    emptyState.className = "mcp-empty-state";
    emptyState.id = "mcp-empty-state";
    const emptyTitle = document.createElement("p");
    emptyTitle.className = "mcp-empty-title";
    emptyTitle.textContent = "No MCP servers configured";
    const emptyHint = document.createElement("p");
    emptyHint.className = "mcp-empty-hint";
    emptyHint.textContent = "Add a server to connect your agent to external tools and data sources.";
    emptyState.appendChild(emptyTitle);
    emptyState.appendChild(emptyHint);
    serverList.appendChild(emptyState);

    serverGroup.appendChild(serverList);

    const addBtn = document.createElement("button");
    addBtn.className = "mcp-add-server-btn";
    addBtn.type = "button";
    addBtn.textContent = "+ Add Server";
    addBtn.addEventListener("click", () => this._addMcpServer());
    serverGroup.appendChild(addBtn);

    section.appendChild(serverGroup);

    // Global connection settings
    const globalGroup = this._createGroup("Connection Settings");
    globalGroup.appendChild(this._createToggleRow("Auto-reconnect", "mcp.global.autoReconnect", true));
    globalGroup.appendChild(this._createNumberRow("Health check interval", "mcp.global.healthCheckInterval", "60", "Seconds between health checks"));
    globalGroup.appendChild(this._createNumberRow("Max reconnect attempts", "mcp.global.maxReconnectAttempts", "3", ""));
    globalGroup.appendChild(this._createNumberRow("Reconnect delay", "mcp.global.reconnectDelay", "5", "Seconds between reconnect attempts"));
    section.appendChild(globalGroup);

    return section;
  },

  _createServerCard(server, index) {
    const card = document.createElement("div");
    card.className = "mcp-server-card";
    card.dataset.serverIndex = index;
    if (!server.enabled) card.classList.add("disabled");

    // Header
    const header = document.createElement("div");
    header.className = "mcp-server-header";

    const expandIcon = document.createElement("span");
    expandIcon.className = "mcp-server-expand";
    expandIcon.textContent = "\u25B6";
    header.appendChild(expandIcon);

    const nameSpan = document.createElement("span");
    nameSpan.className = "mcp-server-name";
    if (server.name) {
      nameSpan.textContent = server.name;
    } else {
      nameSpan.textContent = "Unnamed server";
      nameSpan.classList.add("unnamed");
    }
    header.appendChild(nameSpan);

    const summary = document.createElement("span");
    summary.className = "mcp-server-summary";
    summary.textContent = server.command || "";
    header.appendChild(summary);

    const actions = document.createElement("div");
    actions.className = "mcp-server-actions";

    // Toggle
    const toggle = document.createElement("label");
    toggle.className = "mcp-toggle";
    const toggleInput = document.createElement("input");
    toggleInput.type = "checkbox";
    toggleInput.checked = server.enabled !== false;
    toggleInput.addEventListener("click", (e) => e.stopPropagation());
    toggleInput.addEventListener("change", () => {
      card.classList.toggle("disabled", !toggleInput.checked);
      this._onMcpServerChange();
    });
    const toggleSlider = document.createElement("span");
    toggleSlider.className = "mcp-toggle-slider";
    toggle.appendChild(toggleInput);
    toggle.appendChild(toggleSlider);
    toggle.addEventListener("click", (e) => e.stopPropagation());
    actions.appendChild(toggle);

    const deleteBtn = document.createElement("button");
    deleteBtn.className = "mcp-delete-btn";
    deleteBtn.type = "button";
    deleteBtn.textContent = "\u2715";
    deleteBtn.title = "Remove server";
    deleteBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      this._removeMcpServer(card);
    });
    actions.appendChild(deleteBtn);

    header.appendChild(actions);

    header.addEventListener("click", () => {
      card.classList.toggle("expanded");
    });

    card.appendChild(header);

    // Body (editor)
    const body = document.createElement("div");
    body.className = "mcp-server-body";

    // Name
    body.appendChild(this._createServerFieldRow("Name", "text", server.name || "", (val) => {
      nameSpan.textContent = val || "Unnamed server";
      nameSpan.classList.toggle("unnamed", !val);
      this._onMcpServerChange();
    }));

    // Command
    body.appendChild(this._createServerFieldRow("Command", "text", server.command || "", () => {
      summary.textContent = body.querySelector('[data-field="command"]').value;
      this._onMcpServerChange();
    }, "command"));

    // Args (list editor)
    body.appendChild(this._createServerArgsEditor(server.args || []));

    // Env (key-value editor)
    body.appendChild(this._createServerEnvEditor(server.env || {}));

    card.appendChild(body);
    return card;
  },

  _createServerFieldRow(label, type, value, onChange, fieldName) {
    const row = document.createElement("div");
    row.className = "form-row";

    const lbl = document.createElement("label");
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement("div");
    field.className = "field";

    const input = document.createElement("input");
    input.type = type;
    input.value = value;
    if (fieldName) input.dataset.field = fieldName;
    input.addEventListener("input", () => onChange(input.value));
    field.appendChild(input);

    row.appendChild(field);
    return row;
  },

  _createServerArgsEditor(args) {
    const row = document.createElement("div");
    row.className = "form-row";

    const lbl = document.createElement("label");
    lbl.textContent = "Arguments";
    row.appendChild(lbl);

    const field = document.createElement("div");
    field.className = "field";

    const editor = document.createElement("div");
    editor.className = "list-editor mcp-args-editor";

    const addBtn = document.createElement("button");
    addBtn.className = "add-btn";
    addBtn.type = "button";
    addBtn.textContent = "+ Add";
    addBtn.addEventListener("click", () => {
      this._addMcpArgItem(editor, "");
    });

    editor.appendChild(addBtn);
    field.appendChild(editor);

    // Populate existing args
    for (const arg of args) {
      this._addMcpArgItem(editor, arg);
    }

    row.appendChild(field);
    return row;
  },

  _addMcpArgItem(editor, value) {
    const addBtn = editor.querySelector(".add-btn");

    const item = document.createElement("div");
    item.className = "list-item";

    const input = document.createElement("input");
    input.type = "text";
    input.value = value;
    input.placeholder = "argument";
    input.addEventListener("input", () => this._onMcpServerChange());

    const removeBtn = document.createElement("button");
    removeBtn.className = "remove-btn";
    removeBtn.textContent = "x";
    removeBtn.type = "button";
    removeBtn.addEventListener("click", () => {
      item.remove();
      this._onMcpServerChange();
    });

    item.appendChild(input);
    item.appendChild(removeBtn);
    editor.insertBefore(item, addBtn);
  },

  _createServerEnvEditor(env) {
    const row = document.createElement("div");
    row.className = "form-row";

    const lbl = document.createElement("label");
    lbl.textContent = "Env vars";
    row.appendChild(lbl);

    const field = document.createElement("div");
    field.className = "field";

    const editor = document.createElement("div");
    editor.className = "kv-editor mcp-env-editor";

    const addBtn = document.createElement("button");
    addBtn.className = "add-btn";
    addBtn.type = "button";
    addBtn.textContent = "+ Add";
    addBtn.addEventListener("click", () => {
      this._addMcpEnvRow(editor, "", "");
    });

    editor.appendChild(addBtn);
    field.appendChild(editor);

    // Populate existing env
    for (const [k, v] of Object.entries(env)) {
      this._addMcpEnvRow(editor, k, v);
    }

    row.appendChild(field);
    return row;
  },

  _addMcpEnvRow(editor, key, value) {
    const addBtn = editor.querySelector(".add-btn");

    const kvRow = document.createElement("div");
    kvRow.className = "kv-row";

    const keyInput = document.createElement("input");
    keyInput.type = "text";
    keyInput.className = "kv-key";
    keyInput.value = key;
    keyInput.placeholder = "KEY";
    keyInput.addEventListener("input", () => this._onMcpServerChange());

    const sep = document.createElement("span");
    sep.className = "kv-sep";
    sep.textContent = "=";

    const valInput = document.createElement("input");
    valInput.type = "text";
    valInput.value = value;
    valInput.placeholder = "value";
    valInput.addEventListener("input", () => this._onMcpServerChange());

    const removeBtn = document.createElement("button");
    removeBtn.className = "kv-remove";
    removeBtn.type = "button";
    removeBtn.textContent = "x";
    removeBtn.addEventListener("click", () => {
      kvRow.remove();
      this._onMcpServerChange();
    });

    kvRow.appendChild(keyInput);
    kvRow.appendChild(sep);
    kvRow.appendChild(valInput);
    kvRow.appendChild(removeBtn);
    editor.insertBefore(kvRow, addBtn);
  },

  _addMcpServer() {
    const list = document.getElementById("mcp-server-list");
    const servers = this._getMcpServersFromSettings();
    const newServer = { name: "", command: "", args: [], enabled: true, env: {} };
    servers.push(newServer);

    const card = this._createServerCard(newServer, servers.length - 1);
    card.classList.add("expanded");
    list.appendChild(card);

    this._updateMcpEmptyState();
    this._onMcpServerChange();

    // Focus the name input
    const nameInput = card.querySelector('.mcp-server-body input[type="text"]');
    if (nameInput) nameInput.focus();
  },

  _removeMcpServer(card) {
    card.remove();
    this._updateMcpEmptyState();
    this._onMcpServerChange();
  },

  _updateMcpEmptyState() {
    const list = document.getElementById("mcp-server-list");
    const emptyState = document.getElementById("mcp-empty-state");
    if (!emptyState) return;
    const hasCards = list.querySelector(".mcp-server-card");
    emptyState.style.display = hasCards ? "none" : "";
  },

  _onMcpServerChange() {
    const list = document.getElementById("mcp-server-list");
    const cards = list.querySelectorAll(".mcp-server-card");
    const servers = [];

    for (const card of cards) {
      const body = card.querySelector(".mcp-server-body");
      const inputs = body.querySelectorAll(':scope > .form-row input[type="text"]');
      const name = inputs[0]?.value || "";
      const command = inputs[1]?.value || "";

      // Args
      const argsEditor = body.querySelector(".mcp-args-editor");
      const argInputs = argsEditor.querySelectorAll(".list-item input");
      const args = Array.from(argInputs).map(i => i.value).filter(Boolean);

      // Env
      const envEditor = body.querySelector(".mcp-env-editor");
      const kvRows = envEditor.querySelectorAll(".kv-row");
      const env = {};
      for (const kvRow of kvRows) {
        const k = kvRow.querySelector(".kv-key")?.value?.trim();
        const v = kvRow.querySelectorAll("input")[1]?.value || "";
        if (k) env[k] = v;
      }

      // Enabled
      const toggle = card.querySelector('.mcp-server-actions input[type="checkbox"]');
      const enabled = toggle ? toggle.checked : true;

      servers.push({ name, command, args, enabled, env });
    }

    this._onFieldChange("mcp.servers", servers);
  },

  _getMcpServersFromSettings() {
    return this._getNestedValue(this._settings, "mcp.servers") || [];
  },

  _createToggleRow(label, key, defaultValue) {
    const row = document.createElement("div");
    row.className = "toggle-row";

    const lbl = document.createElement("label");
    lbl.textContent = label;
    row.appendChild(lbl);

    const toggle = document.createElement("label");
    toggle.className = "mcp-toggle";

    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = defaultValue;
    input.dataset.key = key;
    input.addEventListener("change", () => this._onFieldChange(key, input.checked));

    const slider = document.createElement("span");
    slider.className = "mcp-toggle-slider";

    toggle.appendChild(input);
    toggle.appendChild(slider);
    row.appendChild(toggle);

    return row;
  },

  _createNumberRow(label, key, placeholder, hint) {
    const row = document.createElement("div");
    row.className = "form-row";

    const lbl = document.createElement("label");
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement("div");
    field.className = "field";

    const input = document.createElement("input");
    input.type = "number";
    input.min = "0";
    input.placeholder = placeholder || "";
    input.dataset.key = key;
    input.addEventListener("input", () => {
      const val = input.value ? parseInt(input.value, 10) : undefined;
      if (val !== undefined && !isNaN(val)) {
        this._onFieldChange(key, val);
      }
    });
    field.appendChild(input);

    if (hint) {
      const hintEl = document.createElement("div");
      hintEl.className = "hint";
      hintEl.textContent = hint;
      field.appendChild(hintEl);
    }

    row.appendChild(field);
    return row;
  },

  // ── Form Helpers ────────────────────────────────────────

  _createSection(id, title) {
    const section = document.createElement("div");
    section.className = "settings-section";
    section.id = `section-${id}`;
    return section;
  },

  _createGroup(title) {
    const group = document.createElement("div");
    group.className = "settings-group";
    const h2 = document.createElement("h2");
    h2.textContent = title;
    group.appendChild(h2);
    return group;
  },

  _createTextRow(label, key, placeholder, hint) {
    const row = document.createElement("div");
    row.className = "form-row";

    const lbl = document.createElement("label");
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement("div");
    field.className = "field";

    const input = document.createElement("input");
    input.type = "text";
    input.placeholder = placeholder || "";
    input.dataset.key = key;
    input.addEventListener("input", () => this._onFieldChange(key, input.value));
    field.appendChild(input);

    if (hint) {
      const hintEl = document.createElement("div");
      hintEl.className = "hint";
      hintEl.textContent = hint;
      field.appendChild(hintEl);
    }

    row.appendChild(field);
    return row;
  },

  _createTextareaRow(label, key, placeholder, hint) {
    const row = document.createElement("div");
    row.className = "form-row";

    const lbl = document.createElement("label");
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement("div");
    field.className = "field";

    const textarea = document.createElement("textarea");
    textarea.placeholder = placeholder || "";
    textarea.dataset.key = key;
    textarea.addEventListener("input", () => this._onFieldChange(key, textarea.value));
    field.appendChild(textarea);

    if (hint) {
      const hintEl = document.createElement("div");
      hintEl.className = "hint";
      hintEl.textContent = hint;
      field.appendChild(hintEl);
    }

    row.appendChild(field);
    return row;
  },

  _createSelectRow(label, key, options, defaultValue) {
    const row = document.createElement("div");
    row.className = "form-row";

    const lbl = document.createElement("label");
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement("div");
    field.className = "field";

    const select = document.createElement("select");
    select.dataset.key = key;
    for (const [value, text] of options) {
      const opt = document.createElement("option");
      opt.value = value;
      opt.textContent = text;
      if (value === defaultValue) opt.selected = true;
      select.appendChild(opt);
    }
    select.addEventListener("change", () => this._onFieldChange(key, select.value));
    field.appendChild(select);

    row.appendChild(field);
    return row;
  },

  _createPasswordRow(label, key, placeholder) {
    const row = document.createElement("div");
    row.className = "form-row";

    const lbl = document.createElement("label");
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement("div");
    field.className = "field";

    const wrapper = document.createElement("div");
    wrapper.className = "password-wrapper";

    const input = document.createElement("input");
    input.type = "password";
    input.placeholder = placeholder || "";
    input.dataset.key = key;
    input.addEventListener("input", () => this._onFieldChange(key, input.value));

    const toggle = document.createElement("button");
    toggle.className = "password-toggle";
    toggle.textContent = "Show";
    toggle.type = "button";
    toggle.addEventListener("click", () => {
      if (input.type === "password") {
        input.type = "text";
        toggle.textContent = "Hide";
      } else {
        input.type = "password";
        toggle.textContent = "Show";
      }
    });

    wrapper.appendChild(input);
    wrapper.appendChild(toggle);
    field.appendChild(wrapper);
    row.appendChild(field);
    return row;
  },

  _createAvatarRow(label, key) {
    const row = document.createElement("div");
    row.className = "avatar-row";

    const lbl = document.createElement("label");
    lbl.textContent = label;
    row.appendChild(lbl);

    const preview = document.createElement("div");
    preview.className = "avatar-preview";
    preview.id = "avatar-preview";
    preview.textContent = "?";
    row.appendChild(preview);

    const actions = document.createElement("div");
    actions.className = "avatar-actions";

    const fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.accept = "image/*";
    fileInput.style.display = "none";
    fileInput.addEventListener("change", () => {
      const file = fileInput.files[0];
      if (!file) return;
      // Limit to 256KB source file
      if (file.size > 256 * 1024) {
        console.warn("Avatar file too large (max 256KB)");
        return;
      }
      const reader = new FileReader();
      reader.onload = () => {
        const dataUrl = reader.result;
        this._setAvatarPreview(preview, dataUrl);
        this._onFieldChange(key, dataUrl);
      };
      reader.readAsDataURL(file);
      // Reset so the same file can be re-selected
      fileInput.value = "";
    });
    actions.appendChild(fileInput);

    const uploadBtn = document.createElement("button");
    uploadBtn.type = "button";
    uploadBtn.textContent = "Upload";
    uploadBtn.addEventListener("click", () => fileInput.click());
    actions.appendChild(uploadBtn);

    const removeBtn = document.createElement("button");
    removeBtn.type = "button";
    removeBtn.className = "avatar-remove-btn";
    removeBtn.textContent = "Remove";
    removeBtn.addEventListener("click", () => {
      this._setAvatarPreview(preview, null);
      this._onFieldChange(key, "");
    });
    actions.appendChild(removeBtn);

    row.appendChild(actions);
    row.dataset.key = key;
    row.dataset.avatarRow = "true";
    return row;
  },

  _setAvatarPreview(preview, dataUrl) {
    if (dataUrl) {
      preview.textContent = "";
      let img = preview.querySelector("img");
      if (!img) {
        img = document.createElement("img");
        img.alt = "Avatar";
        preview.appendChild(img);
      }
      img.src = dataUrl;
    } else {
      preview.textContent = "?";
      const img = preview.querySelector("img");
      if (img) img.remove();
    }
  },

  _createListRow(label, key) {
    const row = document.createElement("div");
    row.className = "form-row";

    const lbl = document.createElement("label");
    lbl.textContent = label;
    row.appendChild(lbl);

    const field = document.createElement("div");
    field.className = "field";

    const editor = document.createElement("div");
    editor.className = "list-editor";
    editor.dataset.key = key;

    const addBtn = document.createElement("button");
    addBtn.className = "add-btn";
    addBtn.textContent = "+ Add";
    addBtn.type = "button";
    addBtn.addEventListener("click", () => {
      this._addListItem(editor, key, "");
    });

    editor.appendChild(addBtn);
    field.appendChild(editor);
    row.appendChild(field);
    return row;
  },

  _addListItem(editor, key, value) {
    const addBtn = editor.querySelector(".add-btn");

    const item = document.createElement("div");
    item.className = "list-item";

    const input = document.createElement("input");
    input.type = "text";
    input.value = value;
    input.addEventListener("input", () => this._onListChange(editor, key));

    const removeBtn = document.createElement("button");
    removeBtn.className = "remove-btn";
    removeBtn.textContent = "x";
    removeBtn.type = "button";
    removeBtn.addEventListener("click", () => {
      item.remove();
      this._onListChange(editor, key);
    });

    item.appendChild(input);
    item.appendChild(removeBtn);
    editor.insertBefore(item, addBtn);
    input.focus();
  },

  _onListChange(editor, key) {
    const items = editor.querySelectorAll(".list-item input");
    const values = Array.from(items).map(i => i.value).filter(Boolean);
    this._onFieldChange(key, values);
  },

  _renderPlaceholderSection(id, title, message) {
    const section = this._createSection(id, title);
    const group = this._createGroup(title);
    const p = document.createElement("p");
    p.className = "section-placeholder";
    p.textContent = message;
    group.appendChild(p);
    section.appendChild(group);
    return section;
  },

  _el(tag, text) {
    const el = document.createElement(tag);
    el.textContent = text;
    return el;
  },

  _getTimezones() {
    // Common timezones
    return [
      ["UTC", "UTC"],
      ["America/New_York", "US Eastern"],
      ["America/Chicago", "US Central"],
      ["America/Denver", "US Mountain"],
      ["America/Los_Angeles", "US Pacific"],
      ["Europe/London", "London"],
      ["Europe/Paris", "Paris"],
      ["Europe/Berlin", "Berlin"],
      ["Europe/Moscow", "Moscow"],
      ["Asia/Tokyo", "Tokyo"],
      ["Asia/Shanghai", "Shanghai"],
      ["Asia/Kolkata", "India"],
      ["Asia/Singapore", "Singapore"],
      ["Australia/Sydney", "Sydney"],
      ["Pacific/Auckland", "Auckland"],
    ];
  },

  // ── Settings Persistence ────────────────────────────────

  async _loadSettings() {
    try {
      const result = await NevofluxPage.sendQuery("contentStore:get", {
        key: "config:settings",
      });
      if (result && result.value) {
        this._settings = result.value;
        this._populateFields();
      }
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  },

  _populateFields() {
    // Text/select/password/checkbox/number inputs
    for (const input of document.querySelectorAll("[data-key]")) {
      const key = input.dataset.key;
      const value = this._getNestedValue(this._settings, key);
      if (value === undefined) continue;

      if (input.dataset.avatarRow) {
        // Avatar row — value is a data URL string
        const preview = input.querySelector(".avatar-preview");
        if (preview && value) {
          this._setAvatarPreview(preview, value);
        }
      } else if (input.classList.contains("list-editor")) {
        // List editor
        if (Array.isArray(value)) {
          for (const item of value) {
            this._addListItem(input, key, item);
          }
        }
      } else if (input.tagName === "SELECT") {
        input.value = value;
      } else if (input.type === "checkbox") {
        input.checked = !!value;
      } else {
        input.value = value;
      }
    }

    // Populate MCP server cards
    this._populateMcpServers();
  },

  _populateMcpServers() {
    const servers = this._getNestedValue(this._settings, "mcp.servers");
    if (!Array.isArray(servers) || servers.length === 0) return;

    const list = document.getElementById("mcp-server-list");
    if (!list) return;

    for (let i = 0; i < servers.length; i++) {
      const card = this._createServerCard(servers[i], i);
      list.appendChild(card);
    }

    this._updateMcpEmptyState();
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
      await NevofluxPage.sendQuery("contentStore:set", {
        key: "config:settings",
        value: this._settings,
      });
      this._showSaveIndicator();
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  },

  _showSaveIndicator() {
    const indicator = document.getElementById("save-indicator");
    indicator.classList.add("visible");
    setTimeout(() => indicator.classList.remove("visible"), 1500);
  },

  _getNestedValue(obj, path) {
    return path.split(".").reduce((o, k) => o?.[k], obj);
  },

  _setNestedValue(obj, path, value) {
    const keys = path.split(".");
    let current = obj;
    for (let i = 0; i < keys.length - 1; i++) {
      if (!current[keys[i]] || typeof current[keys[i]] !== "object") {
        current[keys[i]] = {};
      }
      current = current[keys[i]];
    }
    current[keys[keys.length - 1]] = value;
  },
};

document.addEventListener("DOMContentLoaded", () => Settings.init());
