/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

/**
 * Canvas micro-app runtime.
 *
 * Loads artifact data from ContentStore via NevofluxChild actor,
 * renders content in a sandboxed iframe, supports preview/edit/fullscreen modes.
 */
const Canvas = {
  _artifactId: null,
  _mode: "preview",
  _debounceTimer: null,
  _iframe: null,
  _artifact: null,
  _vendorCache: {},
  _cmView: null,

  // SDK script injected into every srcdoc iframe for postMessage bridge
  _SDK_SCRIPT: `<script>
(function() {
  var _reqId = 0;
  var _pending = {};
  var _agentSessions = {};

  window.addEventListener("message", function(e) {
    if (!e.data || !e.data._nevoflux) return;

    // Standard request-response
    if (e.data._reqId != null) {
      var cb = _pending[e.data._reqId];
      if (cb) {
        delete _pending[e.data._reqId];
        if (e.data.error) cb.reject(new Error(e.data.error));
        else cb.resolve(e.data.result);
      }
      return;
    }

    // Agent push message (streaming)
    if (e.data._agentPush && e.data.sessionId) {
      var session = _agentSessions[e.data.sessionId];
      if (!session) return;
      var msg = e.data.message;
      var msgType = msg && msg.type;

      if (msgType === "stream_chunk") {
        var delta = (msg.payload && msg.payload.content) || "";
        session.text += delta;
        if (session.onStream) {
          try { session.onStream({ type: "text", delta: delta }); } catch(err) {}
        }
      } else if (msgType === "content_block" || msgType === "browser_tool_result") {
        var toolResult = msg.payload || {};
        session.toolResults.push(toolResult);
        if (session.onToolResult) {
          try { session.onToolResult(toolResult); } catch(err) {}
        }
      } else if (msgType === "agent_state") {
        var status = (msg.payload && (msg.payload.state || msg.payload.status)) || "unknown";
        if (session.onState) {
          try { session.onState({ status: status }); } catch(err) {}
        }
      } else if (msgType === "session:end") {
        var result = {
          text: session.text,
          toolResults: session.toolResults,
          sessionId: e.data.sessionId
        };
        session.resolve(result);
        delete _agentSessions[e.data.sessionId];
      } else if (msgType === "session:error") {
        session.reject(new Error((msg.payload && msg.payload.error) || "Agent error"));
        delete _agentSessions[e.data.sessionId];
      }
    }
  });

  function request(method, args) {
    return new Promise(function(resolve, reject) {
      var id = ++_reqId;
      _pending[id] = { resolve: resolve, reject: reject };
      window.parent.postMessage({
        _nevoflux: true,
        _reqId: id,
        method: method,
        args: args
      }, "*");
    });
  }

  window.NevofluxSDK = {
    callTool: function(action, params) {
      return request("callTool", { action: action, params: params || {} });
    },
    agent: {
      chat: function(message, options) {
        options = options || {};
        var sessionId = options.sessionId || null;

        return new Promise(function(resolve, reject) {
          request("agent.chat", {
            message: message,
            sessionId: sessionId
          }).then(function(res) {
            var sid = res && res.sessionId;
            if (!sid) {
              reject(new Error("No sessionId returned"));
              return;
            }
            _agentSessions[sid] = {
              resolve: resolve,
              reject: reject,
              text: "",
              toolResults: [],
              onStream: options.onStream || null,
              onToolResult: options.onToolResult || null,
              onState: options.onState || null
            };
          }).catch(reject);
        });
      },
      cancel: function(sessionId) {
        if (_agentSessions[sessionId]) {
          _agentSessions[sessionId].reject(new Error("Cancelled"));
          delete _agentSessions[sessionId];
        }
        return request("agent.cancel", { sessionId: sessionId });
      },
      sendCommand: function(command, params) {
        return request("agent.sendCommand", { command: command, params: params });
      }
    },
    sidebar: {
      open: function() { return request("sidebar.open", {}); },
      send: function(message) { return request("sidebar.send", { message: message }); },
      notify: function(type, data) {
        return request("sidebar.notify", { type: type, data: data });
      }
    },
    storage: {
      get: function(key) { return request("storage.get", { key: key }); },
      set: function(key, value) { return request("storage.set", { key: key, value: value }); },
      delete: function(key) { return request("storage.delete", { key: key }); },
      query: function(prefix) { return request("storage.query", { prefix: prefix }); }
    },
    system: {
      getInfo: function() { return request("system.getInfo", {}); }
    }
  };
})();
<\/script>`,

  init() {
    // window.location retains original nevoflux:// URL even though content
    // loads via chrome:// protocol handler rewrite.  The ID lives in the path:
    //   nevoflux://canvas/{id}  →  hostname="canvas", pathname="/{id}"
    const url = new URL(window.location.href);
    this._artifactId = NevofluxPage.getParam("id")
      || url.pathname.replace(/^\//, "")
      || null;
    this._mode = NevofluxPage.getParam("mode", "preview");

    console.error(`[Canvas] init: id=${this._artifactId}, mode=${this._mode}, url=${window.location.href}`);

    if (!this._artifactId) {
      console.error(`[Canvas] ERROR: No artifact ID. location.search=${window.location.search}`);
      this._showEmpty("No artifact ID specified");
      return;
    }

    this._setupToolbar();
    this._setupBridge();
    this._loadArtifact();
  },

  // ── Toolbar ─────────────────────────────────────────────

  _setupToolbar() {
    document.getElementById("btn-preview").addEventListener("click", () => {
      this._switchMode("preview");
    });
    document.getElementById("btn-edit").addEventListener("click", () => {
      this._switchMode("edit");
    });
    document.getElementById("btn-fullscreen").addEventListener("click", () => {
      this._switchMode("fullscreen");
    });
    document.getElementById("btn-copy").addEventListener("click", () => {
      this._copyCode();
    });
    document.getElementById("btn-export-html").addEventListener("click", () => {
      this._exportHtml();
    });
    document.getElementById("btn-export-source").addEventListener("click", () => {
      this._exportSource();
    });
    document.getElementById("btn-share").addEventListener("click", () => {
      this._shareLink();
    });

    this._highlightModeButton(this._mode);
  },

  _highlightModeButton(mode) {
    for (const btn of document.querySelectorAll(".toolbar-btn")) {
      btn.classList.remove("active");
    }
    const btnId = `btn-${mode}`;
    const btn = document.getElementById(btnId);
    if (btn) {
      btn.classList.add("active");
    }
  },

  _switchMode(mode) {
    this._mode = mode;
    this._highlightModeButton(mode);

    // Update URL without reload (skip for nevoflux:// — replaceState is blocked)
    try {
      const url = new URL(window.location.href);
      url.searchParams.set("mode", mode);
      history.replaceState(null, "", url.toString());
    } catch (e) {
      // nevoflux:// protocol doesn't support history.replaceState
    }

    // Handle fullscreen body class
    document.body.classList.toggle("fullscreen", mode === "fullscreen");

    // Re-render with current artifact
    if (this._artifact) {
      if (mode === "edit") {
        this._enterEditMode();
      } else {
        this._exitEditMode();
        this._render(this._artifact);
      }
    }
  },

  async _copyCode() {
    if (!this._artifact?.content) return;
    try {
      // In chrome:// context, use the clipboard API
      await navigator.clipboard.writeText(this._artifact.content);
      const btn = document.getElementById("btn-copy");
      const original = btn.textContent;
      btn.textContent = "Copied!";
      setTimeout(() => { btn.textContent = original; }, 1500);
    } catch (e) {
      console.error("Failed to copy:", e);
    }
  },

  // ── PostMessage Bridge ─────────────────────────────────

  _setupBridge() {
    window.addEventListener("message", async (event) => {
      const msg = event.data;
      if (!msg || !msg._nevoflux || msg._reqId == null) return;

      // Only accept messages from our artifact iframe
      if (!this._iframe || event.source !== this._iframe.contentWindow) return;

      let result, error;
      try {
        result = await this._handleBridgeCall(msg.method, msg.args);
      } catch (e) {
        console.error("[Canvas] Bridge call error:", msg.method, e);
        error = e.message || String(e);
      }

      // Send response back to iframe
      if (this._iframe && this._iframe.contentWindow) {
        this._iframe.contentWindow.postMessage({
          _nevoflux: true,
          _reqId: msg._reqId,
          result,
          error,
        }, "*");
      }
    });
  },

  async _handleBridgeCall(method, args) {
    console.info("[Canvas] _handleBridgeCall:", method, JSON.stringify(args).substring(0, 200));

    // Wait briefly for NevofluxBridge if not yet injected (race with DOMDocElementInserted)
    let bridge = window.NevofluxBridge;
    if (!bridge) {
      console.info("[Canvas] NevofluxBridge not found, waiting...");
      for (let i = 0; i < 10; i++) {
        await new Promise(r => setTimeout(r, 100));
        bridge = window.NevofluxBridge;
        if (bridge) break;
      }
    }
    if (!bridge) throw new Error("NevofluxBridge not available");

    // Diagnostic: log available bridge methods
    console.info("[Canvas] Bridge available. Keys:", Object.keys(bridge),
      "callTool:", typeof bridge.callTool,
      "agent:", typeof bridge.agent,
      "sidebar:", typeof bridge.sidebar,
      "storage:", typeof bridge.storage,
      "system:", typeof bridge.system);

    switch (method) {
      case "callTool":
        console.info("[Canvas] Calling bridge.callTool:", args.action);
        return bridge.callTool(args.action, args.params);
      case "agent.chat": {
        const chatMsg = args.message || args;
        const msgStr = typeof chatMsg === "string" ? chatMsg : JSON.stringify(chatMsg);
        const sessionId = args.sessionId || `cs_${crypto.randomUUID()}`;

        console.info("[Canvas] Sending agent:chat via bridge, sessionId:", sessionId, "msg:", msgStr.substring(0, 100));

        // Step 1: Subscribe FIRST — prevents race where agent responds before subscription exists
        await NevofluxPage.sendQuery("agent:subscribe", { sessionId });

        // Step 2: THEN send to background.js which forwards to agent
        await NevofluxPage.sendQuery("bridge:request", {
          type: "agent:chat",
          payload: { message: msgStr, sessionId }
        });

        return { sessionId };
      }
      case "agent.cancel": {
        const { sessionId } = args;
        if (sessionId) {
          NevofluxPage.sendMessage("agent:unsubscribe", { sessionId });
          await NevofluxPage.sendQuery("bridge:request", {
            type: "agent:cancel",
            payload: { sessionId }
          });
        }
        return { success: true };
      }
      case "sidebar.send": {
        const chatMsg = args.message || args;
        const msgStr = typeof chatMsg === "string" ? chatMsg : JSON.stringify(chatMsg);
        console.info("[Canvas] Sending sidebar:sendMessage via bridge, msg:", msgStr.substring(0, 100));
        const res = await NevofluxPage.sendQuery("bridge:request", {
          type: "sidebar:sendMessage", payload: { message: msgStr }
        });
        return res;
      }
      case "agent.sendCommand":
        return bridge.agent.sendCommand(args.command, args.params);
      case "sidebar.open":
        return NevofluxPage.sendQuery("bridge:request", {
          type: "sidebar:open", payload: {}
        });
      case "sidebar.notify":
        return bridge.sidebar.notify(args.type, args.data);
      case "storage.get": {
        const res = await bridge.storage.get(args.key);
        return res?.value ?? null;
      }
      case "storage.set": {
        await bridge.storage.set(args.key, args.value);
        return true;
      }
      case "storage.delete": {
        const res = await bridge.storage.delete(args.key);
        return res?.success ?? false;
      }
      case "storage.query": {
        const res = await bridge.storage.query(args.prefix);
        return res?.results ?? [];
      }
      case "system.getInfo":
        return bridge.system.getInfo();
      default:
        throw new Error("Unknown method: " + method);
    }
  },

  // ── Export & Share ─────────────────────────────────────

  async _exportHtml() {
    if (!this._artifact?.content) return;
    const type = this._artifact.type || "html";
    let html;

    if (type === "html" || type === "svg") {
      html = this._artifact.content.trim().startsWith("<!DOCTYPE")
        ? this._artifact.content
        : `<!DOCTYPE html>\n<html>\n<head><meta charset="utf-8"><title>${this._escapeHtml(this._artifact.title || "Artifact")}</title></head>\n<body>\n${this._artifact.content}\n</body>\n</html>`;
    } else if (type === "react") {
      html = await this._buildReactStandaloneHtml(this._artifact);
    } else if (type === "markdown") {
      html = await this._buildMarkdownStandaloneHtml(this._artifact);
    } else {
      html = `<!DOCTYPE html>\n<html>\n<head><meta charset="utf-8"><title>${this._escapeHtml(this._artifact.title || "Artifact")}</title></head>\n<body><pre>${this._escapeHtml(this._artifact.content)}</pre></body>\n</html>`;
    }

    this._downloadFile(html, `${this._artifact.title || "artifact"}.html`, "text/html");
  },

  _exportSource() {
    if (!this._artifact?.content) return;
    const extMap = { html: "html", react: "jsx", markdown: "md", svg: "svg", mermaid: "mmd", css: "css", js: "js" };
    const ext = extMap[this._artifact.type] || "txt";
    const filename = `${this._artifact.title || "artifact"}.${ext}`;
    const mime = ext === "html" ? "text/html" : ext === "md" ? "text/markdown" : "text/plain";
    this._downloadFile(this._artifact.content, filename, mime);
  },

  async _shareLink() {
    const url = `nevoflux://canvas/${this._artifactId}`;
    try {
      await navigator.clipboard.writeText(url);
      const btn = document.getElementById("btn-share");
      const original = btn.textContent;
      btn.textContent = "Copied!";
      setTimeout(() => { btn.textContent = original; }, 1500);
    } catch (e) {
      console.error("Failed to copy link:", e);
    }
  },

  _downloadFile(content, filename, mimeType) {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  },

  async _buildReactStandaloneHtml(artifact) {
    const [react, reactDom, babel] = await Promise.all([
      this._fetchVendor("chrome://nevoflux/content/vendor/react.production.min.js"),
      this._fetchVendor("chrome://nevoflux/content/vendor/react-dom.production.min.js"),
      this._fetchVendor("chrome://nevoflux/content/vendor/babel.min.js"),
    ]);

    const title = this._escapeHtml(artifact.title || "Artifact");
    return `<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>${title}</title>
<style>body{margin:0;font-family:system-ui,sans-serif;}</style>
<script>${react}<\/script>
<script>${reactDom}<\/script>
<script>${babel}<\/script>
</head><body>
<div id="root"></div>
<script type="text/babel" data-type="module">
${artifact.content}
const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(React.createElement(typeof App !== 'undefined' ? App : () => React.createElement('div', null, 'No App component found')));
<\/script>
</body></html>`;
  },

  async _buildMarkdownStandaloneHtml(artifact) {
    const [markdownIt, highlightJs] = await Promise.all([
      this._fetchVendor("chrome://nevoflux/content/vendor/markdown-it.min.js"),
      this._fetchVendor("chrome://nevoflux/content/vendor/highlight.min.js"),
    ]);

    const title = this._escapeHtml(artifact.title || "Artifact");
    return `<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>${title}</title>
<style>
body { margin: 24px; font-family: system-ui, sans-serif; line-height: 1.6; color: #1d1d1f; }
pre { background: #f5f5f7; padding: 16px; border-radius: 8px; overflow-x: auto; }
code { font-family: "Fira Code", monospace; font-size: 13px; }
blockquote { border-left: 3px solid #d2d2d7; margin: 0; padding-left: 16px; color: #6e6e73; }
img { max-width: 100%; }
table { border-collapse: collapse; width: 100%; }
th, td { border: 1px solid #d2d2d7; padding: 8px; text-align: left; }
th { background: #f5f5f7; }
</style>
<script>${markdownIt}<\/script>
<script>${highlightJs}<\/script>
</head><body>
<div id="content"></div>
<script>
var md = window.markdownit({
  html: true,
  highlight: function(str, lang) {
    if (lang && hljs.getLanguage(lang)) {
      try { return hljs.highlight(str, { language: lang }).value; } catch {}
    }
    return '';
  }
});
document.getElementById('content').innerHTML = md.render(${JSON.stringify(artifact.content)});
<\/script>
</body></html>`;
  },

  // ── Data Loading ────────────────────────────────────────

  async _loadArtifact() {
    try {
      console.error(`[Canvas] _loadArtifact: key=canvas:${this._artifactId}`);
      const result = await NevofluxPage.sendQuery("contentStore:get", {
        key: `canvas:${this._artifactId}`,
      });

      console.error(`[Canvas] _loadArtifact result: hasValue=${!!(result && result.value)}, type=${result?.value?.type}, contentLen=${result?.value?.content?.length}, state=${result?.value?.state}`);

      if (result && result.value) {
        this._artifact = result.value;
        this._onArtifactUpdate(result.value);
      } else {
        console.error(`[Canvas] _loadArtifact: Artifact not found!`);
        this._showEmpty("Artifact not found");
        this._updateStatus("Not found");
      }

      // Subscribe to future updates
      NevofluxPage.sendMessage("contentStore:subscribe", {
        key: `canvas:${this._artifactId}`,
      });
    } catch (e) {
      console.error(`[Canvas] _loadArtifact ERROR: ${e}`);
      console.error("Failed to load artifact:", e);
      this._showEmpty("Failed to load artifact");
    }
  },

  _onArtifactUpdate(artifact) {
    if (!artifact) {
      this._showEmpty("Artifact deleted");
      return;
    }

    this._artifact = artifact;

    // Update toolbar
    document.getElementById("artifact-title").textContent =
      artifact.title || "Untitled";

    const stateLabels = {
      streaming: "Generating...",
      complete: "Ready",
      error: "Error",
    };
    this._updateStatus(stateLabels[artifact.state] || artifact.state);

    // Debounce rendering (300ms for streaming, immediate for complete)
    if (this._debounceTimer) {
      clearTimeout(this._debounceTimer);
    }

    if (artifact.state === "complete") {
      this._render(artifact);
    } else {
      this._debounceTimer = setTimeout(() => {
        this._render(artifact);
      }, 300);
    }
  },

  // ── Rendering ───────────────────────────────────────────

  _render(artifact) {
    console.error(`[Canvas] _render: type=${artifact.type}, contentLen=${artifact.content?.length}, state=${artifact.state}, mode=${this._mode}`);

    if (this._mode === "edit") {
      this._updateEditPreview(artifact);
      return;
    }

    const viewport = document.getElementById("viewport");
    const emptyState = document.getElementById("empty-state");

    if (!artifact.content) {
      console.error(`[Canvas] _render: content is empty/falsy`);
      this._showEmpty("Empty artifact");
      return;
    }

    // Hide empty state
    if (emptyState) {
      emptyState.style.display = "none";
    }

    // Normalize type (handle both short types and MIME types)
    const type = this._normalizeType(artifact.type);
    console.error(`[Canvas] _render: normalizedType=${type}`);
    switch (type) {
      case "html":
        // Auto-detect React HTML: if HTML contains React CDN scripts or <script type="text/babel">,
        // extract JSX code and render via _renderReact which inlines vendor scripts.
        if (this._isReactHtml(artifact.content)) {
          console.info("[Canvas] Auto-detected React in HTML content, using _renderReact");
          const jsx = this._extractReactCode(artifact.content);
          this._renderReact(viewport, jsx);
        } else {
          this._renderSrcdoc(viewport, artifact.content);
        }
        break;
      case "react":
        this._renderReact(viewport, artifact.content);
        break;
      case "markdown":
        this._renderMarkdown(viewport, artifact.content);
        break;
      case "svg":
        this._renderSrcdoc(viewport, artifact.content);
        break;
      case "mermaid":
        this._renderMermaid(viewport, artifact.content);
        break;
      default:
        this._showEmpty(`Unsupported type: ${artifact.type}`);
    }
  },

  /**
   * Render raw HTML/SVG content in a sandboxed iframe.
   * Injects NevofluxSDK postMessage bridge into every srcdoc.
   */
  _renderSrcdoc(viewport, htmlContent) {
    if (this._iframe) {
      this._iframe.remove();
    }
    this._iframe = document.createElement("iframe");
    this._iframe.setAttribute("sandbox", "allow-scripts allow-forms");

    // Inject NevofluxSDK into srcdoc using indexOf+slice (avoid String.replace
    // which can interpret $-patterns in the replacement string).
    let injected = htmlContent;
    const sdk = this._SDK_SCRIPT;
    let idx = injected.indexOf("</head>");
    if (idx !== -1) {
      injected = injected.slice(0, idx) + sdk + injected.slice(idx);
    } else {
      idx = injected.indexOf("<body");
      if (idx !== -1) {
        injected = injected.slice(0, idx) + sdk + injected.slice(idx);
      } else {
        injected = sdk + injected;
      }
    }

    console.error(`[Canvas] _renderSrcdoc: inputLen=${htmlContent.length}, injectedLen=${injected.length}, sdkLen=${sdk.length}`);
    this._iframe.srcdoc = injected;
    viewport.appendChild(this._iframe);
  },

  /**
   * Render React JSX code. Fetches vendor libs from chrome:// and inlines them
   * into srcdoc since the sandbox has no allow-same-origin.
   */
  async _renderReact(viewport, code) {
    const [react, reactDom, babel] = await Promise.all([
      this._fetchVendor("chrome://nevoflux/content/vendor/react.production.min.js"),
      this._fetchVendor("chrome://nevoflux/content/vendor/react-dom.production.min.js"),
      this._fetchVendor("chrome://nevoflux/content/vendor/babel.min.js"),
    ]);

    const srcdoc = `<!DOCTYPE html>
<html><head><meta charset="utf-8">
<style>body{margin:0;font-family:system-ui,sans-serif;}</style>
<script>${react}<\/script>
<script>${reactDom}<\/script>
<script>${babel}<\/script>
</head><body>
<div id="root"></div>
<script type="text/babel" data-type="module">
${code}
const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(React.createElement(typeof App !== 'undefined' ? App : () => React.createElement('div', null, 'No App component found')));
<\/script>
</body></html>`;

    this._renderSrcdoc(viewport, srcdoc);
  },

  /**
   * Render Markdown content using markdown-it.
   */
  async _renderMarkdown(viewport, markdown) {
    const markdownIt = await this._fetchVendor(
      "chrome://nevoflux/content/vendor/markdown-it.min.js"
    );
    const highlightJs = await this._fetchVendor(
      "chrome://nevoflux/content/vendor/highlight.min.js"
    );

    const srcdoc = `<!DOCTYPE html>
<html><head><meta charset="utf-8">
<style>
body { margin: 24px; font-family: system-ui, sans-serif; line-height: 1.6; color: #1d1d1f; }
pre { background: #f5f5f7; padding: 16px; border-radius: 8px; overflow-x: auto; }
code { font-family: "Fira Code", monospace; font-size: 13px; }
blockquote { border-left: 3px solid #d2d2d7; margin: 0; padding-left: 16px; color: #6e6e73; }
img { max-width: 100%; }
table { border-collapse: collapse; width: 100%; }
th, td { border: 1px solid #d2d2d7; padding: 8px; text-align: left; }
th { background: #f5f5f7; }
</style>
<script>${markdownIt}<\/script>
<script>${highlightJs}<\/script>
</head><body>
<div id="content"></div>
<script>
var md = window.markdownit({
  html: true,
  highlight: function(str, lang) {
    if (lang && hljs.getLanguage(lang)) {
      try { return hljs.highlight(str, { language: lang }).value; } catch {}
    }
    return '';
  }
});
document.getElementById('content').innerHTML = md.render(${JSON.stringify(markdown)});
<\/script>
</body></html>`;

    this._renderSrcdoc(viewport, srcdoc);
  },

  /**
   * Render Mermaid diagrams.
   */
  async _renderMermaid(viewport, code) {
    const mermaid = await this._fetchVendor(
      "chrome://nevoflux/content/vendor/mermaid.min.js"
    );

    const srcdoc = `<!DOCTYPE html>
<html><head><meta charset="utf-8">
<style>body{margin:24px;display:flex;justify-content:center;}</style>
<script>${mermaid}<\/script>
</head><body>
<div class="mermaid">${this._escapeHtml(code)}</div>
<script>mermaid.initialize({ startOnLoad: true, theme: 'default' });<\/script>
</body></html>`;

    this._renderSrcdoc(viewport, srcdoc);
  },

  /**
   * Fetch a chrome:// vendor file and cache it for srcdoc inlining.
   */
  async _fetchVendor(url) {
    if (this._vendorCache[url]) {
      return this._vendorCache[url];
    }
    try {
      const resp = await fetch(url);
      const text = await resp.text();
      this._vendorCache[url] = text;
      return text;
    } catch (e) {
      console.error(`Failed to fetch vendor: ${url}`, e);
      return `/* Failed to load: ${url} */`;
    }
  },

  _escapeHtml(str) {
    return str
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  },

  // ── Edit Mode ───────────────────────────────────────────

  _enterEditMode() {
    const viewport = document.getElementById("viewport");
    const emptyState = document.getElementById("empty-state");
    if (emptyState) emptyState.style.display = "none";
    if (this._iframe) {
      this._iframe.remove();
      this._iframe = null;
    }

    // Destroy previous CodeMirror instance
    if (this._cmView) {
      this._cmView.destroy();
      this._cmView = null;
    }

    // Create split view
    viewport.innerHTML = "";
    const split = document.createElement("div");
    split.className = "canvas-split";
    split.id = "edit-split";

    const editorPane = document.createElement("div");
    editorPane.className = "editor-pane";
    editorPane.id = "code-editor-pane";

    const previewPane = document.createElement("div");
    previewPane.className = "preview-pane";
    previewPane.id = "edit-preview";

    split.appendChild(editorPane);
    split.appendChild(previewPane);
    viewport.appendChild(split);

    // Detect dark theme
    const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;

    // Live preview debounce
    let editDebounce = null;

    // Create CodeMirror editor (falls back to textarea if bundle not loaded)
    if (window.CodeMirrorFactory) {
      this._cmView = window.CodeMirrorFactory.create(editorPane, {
        value: this._artifact?.content || "",
        language: this._artifact?.type || "html",
        dark: isDark,
        onChange: (value) => {
          if (editDebounce) clearTimeout(editDebounce);
          editDebounce = setTimeout(() => {
            this._updateEditPreviewFromCode(value);
          }, 300);
        },
      });
    } else {
      // Fallback: plain textarea
      const textarea = document.createElement("textarea");
      textarea.id = "code-editor";
      textarea.value = this._artifact?.content || "";
      textarea.spellcheck = false;
      textarea.addEventListener("keydown", (e) => {
        if (e.key === "Tab") {
          e.preventDefault();
          const start = textarea.selectionStart;
          const end = textarea.selectionEnd;
          textarea.value =
            textarea.value.substring(0, start) + "  " + textarea.value.substring(end);
          textarea.selectionStart = textarea.selectionEnd = start + 2;
        }
      });
      textarea.addEventListener("input", () => {
        if (editDebounce) clearTimeout(editDebounce);
        editDebounce = setTimeout(() => {
          this._updateEditPreviewFromCode(textarea.value);
        }, 300);
      });
      editorPane.appendChild(textarea);
    }

    // Render initial preview
    if (this._artifact?.content) {
      this._updateEditPreviewFromCode(this._artifact.content);
    }
  },

  _exitEditMode() {
    const split = document.getElementById("edit-split");
    if (split) {
      // Save edits back to ContentStore
      let newContent = null;
      if (this._cmView) {
        newContent = window.CodeMirrorFactory.getValue(this._cmView);
      } else {
        const editor = document.getElementById("code-editor");
        if (editor) newContent = editor.value;
      }

      if (newContent != null && this._artifact && newContent !== this._artifact.content) {
        this._artifact.content = newContent;
        NevofluxPage.sendMessage("contentStore:set", {
          key: `canvas:${this._artifactId}`,
          value: this._artifact,
        });
      }

      // Destroy CodeMirror instance
      if (this._cmView) {
        this._cmView.destroy();
        this._cmView = null;
      }

      split.remove();
    }
  },

  _updateEditPreview(artifact) {
    // External update while editing -- don't overwrite user edits,
    // just update the preview pane
    this._updateEditPreviewFromCode(artifact.content);
  },

  _updateEditPreviewFromCode(code) {
    const previewPane = document.getElementById("edit-preview");
    if (!previewPane || !this._artifact) return;

    // Create a temporary artifact for rendering
    const tempArtifact = { ...this._artifact, content: code };

    // Remove old iframe in preview pane
    const oldIframe = previewPane.querySelector("iframe");
    if (oldIframe) oldIframe.remove();

    // Render into preview pane
    switch (tempArtifact.type) {
      case "html":
      case "svg": {
        const iframe = document.createElement("iframe");
        iframe.setAttribute("sandbox", "allow-scripts allow-forms");
        iframe.srcdoc = code;
        previewPane.appendChild(iframe);
        break;
      }
      default:
        // For complex types, re-use main render path
        this._renderInContainer(previewPane, tempArtifact);
    }
  },

  async _renderInContainer(container, artifact) {
    // Simple fallback: create iframe with srcdoc
    const iframe = document.createElement("iframe");
    iframe.setAttribute("sandbox", "allow-scripts allow-forms");
    iframe.srcdoc = artifact.content;
    container.appendChild(iframe);
  },

  // ── Empty State ─────────────────────────────────────────

  _showEmpty(message) {
    const emptyState = document.getElementById("empty-state");
    if (emptyState) {
      emptyState.textContent = message;
      emptyState.style.display = "flex";
    }
    if (this._iframe) {
      this._iframe.remove();
      this._iframe = null;
    }
  },

  _updateStatus(text) {
    document.getElementById("artifact-status").textContent = text;
  },

  /**
   * Normalize MIME type to short canvas renderer type.
   */
  _normalizeType(rawType) {
    if (!rawType) return "html";
    const MIME_MAP = {
      "text/html": "html",
      "text/markdown": "markdown",
      "text/svg+xml": "svg",
      "image/svg+xml": "svg",
      "application/javascript": "react",
      "text/jsx": "react",
    };
    return MIME_MAP[rawType] || rawType;
  },

  /**
   * Detect if HTML content is actually a React app loaded via CDN scripts.
   * These won't work in sandboxed iframes, so we need to use _renderReact instead.
   */
  _isReactHtml(html) {
    return /text\/babel/i.test(html) ||
      (/react/i.test(html) && /ReactDOM/i.test(html) && /createRoot|render/i.test(html));
  },

  /**
   * Extract JSX/React code from an HTML page that loads React via CDN.
   * Returns just the JSX code for use with _renderReact().
   */
  _extractReactCode(html) {
    // Extract content from <script type="text/babel"> blocks
    const babelMatch = html.match(/<script[^>]*type\s*=\s*["']text\/babel["'][^>]*>([\s\S]*?)<\/script>/i);
    if (babelMatch) {
      return babelMatch[1].trim();
    }

    // Fallback: extract from last <script> block (likely the app code)
    const scripts = [...html.matchAll(/<script(?![^>]*src)[^>]*>([\s\S]*?)<\/script>/gi)];
    if (scripts.length > 0) {
      const last = scripts[scripts.length - 1];
      return last[1].trim();
    }

    // Last resort: return as-is, _renderReact will wrap it
    return html;
  },
};

// Handle actor messages for ContentStore subscription updates and agent push
window.addEventListener("NevofluxMessage", (event) => {
  const detail = event.detail;
  const { type, key, value } = detail;

  // ContentStore updates (existing)
  if (
    type === "contentStore:update" &&
    key === `canvas:${Canvas._artifactId}`
  ) {
    console.error(`[Canvas] ContentStore update received: contentLen=${value?.content?.length}, state=${value?.state}`);
    Canvas._onArtifactUpdate(value);
    return;
  }

  // Agent push messages → forward to artifact iframe
  if (detail.sessionId && detail.message && Canvas._iframe?.contentWindow) {
    Canvas._iframe.contentWindow.postMessage({
      _nevoflux: true,
      _agentPush: true,
      sessionId: detail.sessionId,
      message: detail.message,
    }, "*");
  }
});

document.addEventListener("DOMContentLoaded", () => Canvas.init());
