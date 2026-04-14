/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/**
 * Canvas micro-app runtime.
 *
 * Loads artifact data from ContentStore via NevofluxChild actor,
 * renders content in a sandboxed iframe, supports preview/edit/fullscreen modes.
 */
const Canvas = {
  _artifactId: null,
  _mode: 'preview',
  _debounceTimer: null,
  _iframe: null,
  _artifact: null,
  _vendorCache: {},
  _cmView: null,

  // SDK script injected into every srcdoc iframe for postMessage bridge
  _SDK_SCRIPT: `<script>
(function() {
  // Polyfill localStorage/sessionStorage for chrome:// origins (NS_ERROR_NOT_AVAILABLE)
  (function() {
    var ok = false;
    try { window.localStorage; ok = true; } catch(_) {}
    if (!ok) {
      var MS = function() { this._d = {}; };
      MS.prototype = {
        getItem: function(k) { return this._d.hasOwnProperty(k) ? this._d[k] : null; },
        setItem: function(k, v) { this._d[k] = String(v); },
        removeItem: function(k) { delete this._d[k]; },
        clear: function() { this._d = {}; },
        key: function(i) { return Object.keys(this._d)[i] || null; },
        get length() { return Object.keys(this._d).length; }
      };
      try { Object.defineProperty(window, "localStorage", { value: new MS(), configurable: true }); } catch(_) {
        try { Object.defineProperty(Window.prototype, "localStorage", { get: function() { return this.__ls || (this.__ls = new MS()); }, configurable: true }); } catch(_) {}
      }
      try { Object.defineProperty(window, "sessionStorage", { value: new MS(), configurable: true }); } catch(_) {
        try { Object.defineProperty(Window.prototype, "sessionStorage", { get: function() { return this.__ss || (this.__ss = new MS()); }, configurable: true }); } catch(_) {}
      }
    }
  })();

  var _reqId = 0;
  var _pending = {};
  var _agentSessions = {};

  window.addEventListener("message", function(e) {
    if (!e.data || !e.data._nevoflux) return;

    // Handle EventBus delivery push messages
    if (e.data.type === 'events:delivery') {
      var handlers = window._nevofluxEventHandlers || {};
      var subId = e.data.payload && e.data.payload.subscription_id;
      if (subId && handlers[subId]) {
        try {
          handlers[subId](e.data.payload.event);
        } catch(handlerErr) {
          console.error('[NevofluxSDK] Event handler error:', handlerErr);
        }
      }
      return;
    }

    // Handle Canvas Tool streaming events
    if (e.data.type === 'canvas:tool:event') {
      var handlers = window._nevofluxToolEventHandlers || {};
      var callId = e.data.payload && e.data.payload.call_id;
      if (callId && handlers[callId]) {
        try {
          handlers[callId](e.data.payload);
        } catch(err) {
          console.error('[NevofluxSDK] Tool event handler error:', err);
        }
        // Clean up on finished/error events
        if (e.data.payload.event_type === 'finished' || e.data.payload.event_type === 'error') {
          delete handlers[callId];
        }
      }
      return;
    }

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
        var attachments = options.attachments || [];

        return new Promise(function(resolve, reject) {
          request("agent.chat", {
            message: message,
            sessionId: sessionId,
            attachments: attachments
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
    },
    events: {
      subscribe: function(patterns, handler, options) {
        options = options || {};
        var _subHandlers = window._nevofluxEventHandlers || {};
        window._nevofluxEventHandlers = _subHandlers;

        return request("events.subscribe", {
          patterns: Array.isArray(patterns) ? patterns : [patterns],
          replay_sticky: options.replaySticky !== false,
          buffer_size: options.bufferSize || 256,
        }).then(function(res) {
          var subId = res.subscriptionId;
          _subHandlers[subId] = handler;
          return {
            subscriptionId: subId,
            unsubscribe: function() {
              delete _subHandlers[subId];
              return request("events.unsubscribe", { subscriptionId: subId });
            }
          };
        });
      },

      publish: function(topic, data, options) {
        options = options || {};
        return request("events.publish", {
          topic: topic,
          data: data,
          delivery: options.delivery || "ephemeral",
        });
      },

      history: function(topic, options) {
        options = options || {};
        return request("events.history", {
          topic: topic,
          limit: options.limit || 100,
          since_ms: options.sinceMs || null,
        });
      },

      waitFor: function(pattern, options) {
        options = options || {};
        var timeoutMs = options.timeoutMs || 30000;

        return new Promise(function(resolve, reject) {
          var timer = setTimeout(function() {
            sub.then(function(s) { s.unsubscribe(); });
            reject(new Error("waitFor timeout after " + timeoutMs + "ms"));
          }, timeoutMs);

          var sub = NevofluxSDK.events.subscribe([pattern], function(event) {
            clearTimeout(timer);
            sub.then(function(s) { s.unsubscribe(); });
            resolve(event);
          }, { replaySticky: true, bufferSize: 1 });
        });
      },

      recover: function() {
        return request("events.recover", {}).then(function(res) {
          if (!res.specs || res.specs.length === 0) return [];
          var recovered = [];
          var promises = res.specs.map(function(spec) {
            return NevofluxSDK.events.subscribe(spec.patterns, null).then(function(sub) {
              recovered.push(sub);
            });
          });
          return Promise.all(promises).then(function() { return recovered; });
        });
      }
    },
    tool: {
      /**
       * List available whitelisted tools.
       * @param {object} [options] - { category: string }
       * @returns {Promise<{ tools: Array }>}
       */
      list: function(options) {
        options = options || {};
        return request("canvas.tool.list", {
          category: options.category || null,
        });
      },

      /**
       * Invoke a whitelisted tool.
       * @param {string} toolName - Tool name (e.g. "ffmpeg.trim")
       * @param {object} params - Parameter key-value pairs
       * @param {object} [options] - { timeoutMs, onEvent }
       * @returns {Promise<object>} - Invocation result
       */
      invoke: function(toolName, params, options) {
        options = options || {};
        var _toolEventHandlers = window._nevofluxToolEventHandlers || {};
        window._nevofluxToolEventHandlers = _toolEventHandlers;

        return request("canvas.tool.invoke", {
          tool_name: toolName,
          params: params || {},
          timeout_ms: options.timeoutMs || null,
        }).then(function(res) {
          var callId = res.callId;
          if (options.onEvent && callId) {
            _toolEventHandlers[callId] = options.onEvent;
          }
          // The actual result comes async via canvas:tool:response
          // For now return the callId for tracking
          return { callId: callId, pending: true };
        });
      }
    },
    // NOTE: share events are published by the daemon side via EventBus topics:
    //   share:created, share:imported, share:extended, share:deleted, share:expiring_soon
    // UI code can subscribe via NevofluxSDK.events.subscribe(['share:*'], ...)
    share: {
      /**
       * Share an artifact. Returns { share_id, share_url, password, expires_at }.
       * SECURITY: password is returned ONCE. Caller must show to user immediately.
       */
      share: function(artifactId, options) {
        options = options || {};
        return request("canvas.share", {
          artifact_id: artifactId,
          ttl_secs: options.ttlSecs || null,
        });
      },

      /**
       * Import a shared canvas with password.
       */
      import: function(shareId, password) {
        return request("canvas.import", {
          share_id: shareId,
          password: password,
        });
      },

      /**
       * Extend a share's TTL.
       */
      extend: function(shareId, extendSecs) {
        return request("canvas.share.extend", {
          share_id: shareId,
          extend_secs: extendSecs || 2592000,
        });
      },

      /**
       * Delete a share.
       */
      delete: function(shareId) {
        return request("canvas.share.delete", {
          share_id: shareId,
        });
      },

      /**
       * List all active shares.
       */
      list: function() {
        return request("canvas.share.list", {});
      },

      /**
       * Copy a password to clipboard with auto-clear (default 60 seconds).
       * Returns a promise that resolves when the password has been cleared.
       */
      copyPasswordWithAutoClear: function(password, timeoutMs) {
        timeoutMs = timeoutMs || 60000;
        return navigator.clipboard.writeText(password).then(function() {
          return new Promise(function(resolve) {
            setTimeout(function() {
              // Attempt to clear by writing empty string
              navigator.clipboard.writeText("").catch(function(){}).finally(function() {
                resolve();
              });
            }, timeoutMs);
          });
        });
      }
    }
  };
})();
</script>`,

  init() {
    // window.location retains original nevoflux:// URL even though content
    // loads via chrome:// protocol handler rewrite.  The ID lives in the path:
    //   nevoflux://canvas/{id}  →  hostname="canvas", pathname="/{id}"
    const url = new URL(window.location.href);
    this._artifactId = NevofluxPage.getParam('id') || url.pathname.replace(/^\//, '') || null;
    this._mode = NevofluxPage.getParam('mode', 'preview');

    // Detect import mode: nevoflux://canvas/?mode=import&share_id=xxx
    // UI code can check window._nevofluxImportShareId and show import dialog.
    try {
      const importShareId = NevofluxPage.getParam('share_id');
      if (this._mode === 'import' && importShareId) {
        window._nevofluxImportShareId = importShareId;
        console.info('[Canvas] Import mode detected, share_id=', importShareId);
      }
    } catch (e) {
      // NevofluxPage may not expose share_id via getParam; ignore
    }

    console.error(
      `[Canvas] init: id=${this._artifactId}, mode=${this._mode}, url=${window.location.href}`
    );

    if (!this._artifactId) {
      console.error(`[Canvas] ERROR: No artifact ID. location.search=${window.location.search}`);
      this._showEmpty('No artifact ID specified');
      return;
    }

    this._setupToolbar();
    this._setupBridge();
    this._loadArtifact();
  },

  // ── Toolbar ─────────────────────────────────────────────

  _setupToolbar() {
    document.getElementById('btn-preview').addEventListener('click', () => {
      this._switchMode('preview');
    });
    document.getElementById('btn-edit').addEventListener('click', () => {
      this._switchMode('edit');
    });
    document.getElementById('btn-copy').addEventListener('click', () => {
      this._copyCode();
    });
    // Split Button: left = default export
    document.getElementById('btn-export-default').addEventListener('click', () => {
      this._exportSource();
    });

    // Split Button: arrow = toggle dropdown
    const arrow = document.getElementById('btn-export-arrow');
    const dropdown = document.getElementById('export-dropdown');

    arrow.addEventListener('click', (e) => {
      e.stopPropagation();
      const isOpen = dropdown.classList.toggle('open');
      arrow.setAttribute('aria-expanded', isOpen);
      if (isOpen) {
        this._updateConditionalFormats();
        const firstItem = dropdown.querySelector('.export-dropdown-item:not([style*="display:none"])');
        if (firstItem) firstItem.focus();
      }
    });

    // Close dropdown on outside click
    document.addEventListener('click', (e) => {
      if (!e.target.closest('#export-container')) {
        dropdown.classList.remove('open');
        arrow.setAttribute('aria-expanded', 'false');
      }
    });

    // Keyboard: Escape to close, Arrow keys to navigate items
    document.addEventListener('keydown', (e) => {
      if (!dropdown.classList.contains('open')) return;
      if (e.key === 'Escape') {
        dropdown.classList.remove('open');
        arrow.setAttribute('aria-expanded', 'false');
        arrow.focus();
        return;
      }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const items = [...dropdown.querySelectorAll('.export-dropdown-item:not([style*="display:none"])')];
        const current = items.indexOf(document.activeElement);
        const next = e.key === 'ArrowDown'
          ? (current + 1) % items.length
          : (current - 1 + items.length) % items.length;
        items[next]?.focus();
      }
    });

    // Dispatch format clicks
    dropdown.addEventListener('click', (e) => {
      const item = e.target.closest('[data-format]');
      if (!item) return;
      dropdown.classList.remove('open');
      arrow.setAttribute('aria-expanded', 'false');
      const format = item.dataset.format;
      const dispatch = {
        source:   () => this._exportSource(),
        html:     () => this._safeExport(this._exportHtml, 'HTML'),
        image:    () => this._safeExport(this._exportImage, 'PNG'),
        pdf:      () => this._exportPdf(),
        docx:     () => this._safeExport(this._exportDocx, 'DOCX'),
        svg:      () => this._safeExport(this._exportSvg, 'SVG'),
        markdown: () => this._safeExport(this._exportMarkdown, 'Markdown'),
        pptx:     () => this._safeExport(this._exportPptx, 'PPTX'),
        xlsx:     () => this._safeExport(this._exportXlsx, 'XLSX'),
        zip:      () => this._safeExport(this._exportZip, 'ZIP'),
      };
      if (dispatch[format]) dispatch[format]();
    });

    document.getElementById('btn-share').addEventListener('click', () => {
      this._shareLink();
    });

    this._highlightModeButton(this._mode);
  },

  _highlightModeButton(mode) {
    for (const btn of document.querySelectorAll('.toolbar-btn')) {
      btn.classList.remove('active');
    }
    const btnId = `btn-${mode}`;
    const btn = document.getElementById(btnId);
    if (btn) {
      btn.classList.add('active');
    }
  },

  _switchMode(mode) {
    this._mode = mode;
    this._highlightModeButton(mode);

    // Update URL without reload (skip for nevoflux:// — replaceState is blocked)
    try {
      const url = new URL(window.location.href);
      url.searchParams.set('mode', mode);
      history.replaceState(null, '', url.toString());
    } catch (e) {
      // nevoflux:// protocol doesn't support history.replaceState
    }

    // Re-render with current artifact
    if (this._artifact) {
      if (mode === 'edit') {
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
      await navigator.clipboard.writeText(this._artifact.content);
      const btn = document.getElementById('btn-copy');
      const originalSvg = btn.innerHTML;
      btn.innerHTML =
        '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>';
      btn.classList.add('success');
      setTimeout(() => {
        btn.innerHTML = originalSvg;
        btn.classList.remove('success');
      }, 1500);
    } catch (e) {
      console.error('Failed to copy:', e);
    }
  },

  // ── PostMessage Bridge ─────────────────────────────────

  _setupBridge() {
    window.addEventListener('message', async (event) => {
      const msg = event.data;
      if (!msg || !msg._nevoflux || msg._reqId == null) return;

      // Only accept messages from our artifact iframe
      if (!this._iframe || event.source !== this._iframe.contentWindow) return;

      let result, error;
      try {
        result = await this._handleBridgeCall(msg.method, msg.args);
      } catch (e) {
        console.error('[Canvas] Bridge call error:', msg.method, e);
        error = e.message || String(e);
      }

      // Send response back to iframe
      if (this._iframe && this._iframe.contentWindow) {
        this._iframe.contentWindow.postMessage(
          {
            _nevoflux: true,
            _reqId: msg._reqId,
            result,
            error,
          },
          '*'
        );
      }
    });
  },

  async _handleBridgeCall(method, args) {
    console.info('[Canvas] _handleBridgeCall:', method, JSON.stringify(args).substring(0, 200));

    // Wait briefly for NevofluxBridge if not yet injected (race with DOMDocElementInserted)
    let bridge = window.NevofluxBridge;
    if (!bridge) {
      console.info('[Canvas] NevofluxBridge not found, waiting...');
      for (let i = 0; i < 10; i++) {
        await new Promise((r) => setTimeout(r, 100));
        bridge = window.NevofluxBridge;
        if (bridge) break;
      }
    }
    if (!bridge) throw new Error('NevofluxBridge not available');

    // Diagnostic: log available bridge methods
    console.info(
      '[Canvas] Bridge available. Keys:',
      Object.keys(bridge),
      'callTool:',
      typeof bridge.callTool,
      'agent:',
      typeof bridge.agent,
      'sidebar:',
      typeof bridge.sidebar,
      'storage:',
      typeof bridge.storage,
      'system:',
      typeof bridge.system
    );

    switch (method) {
      case 'callTool':
        console.info('[Canvas] Calling bridge.callTool:', args.action);
        return bridge.callTool(args.action, args.params);
      case 'agent.chat': {
        const chatMsg = args.message || args;
        const msgStr = typeof chatMsg === 'string' ? chatMsg : JSON.stringify(chatMsg);
        const sessionId = args.sessionId || `cs_${crypto.randomUUID()}`;

        console.info(
          '[Canvas] Sending agent:chat via bridge, sessionId:',
          sessionId,
          'msg:',
          msgStr.substring(0, 100)
        );

        // Step 1: Subscribe FIRST — prevents race where agent responds before subscription exists
        await NevofluxPage.sendQuery('agent:subscribe', { sessionId });

        // Step 2: THEN send to background.js which forwards to agent
        // Include attachments if provided (images, files, directories)
        const chatPayload = { message: msgStr, sessionId };
        if (args.attachments && args.attachments.length > 0) {
          chatPayload.attachments = args.attachments;
        }
        await NevofluxPage.sendQuery('bridge:request', {
          type: 'agent:chat',
          payload: chatPayload,
        });

        return { sessionId };
      }
      case 'agent.cancel': {
        const { sessionId } = args;
        if (sessionId) {
          NevofluxPage.sendMessage('agent:unsubscribe', { sessionId });
          await NevofluxPage.sendQuery('bridge:request', {
            type: 'agent:cancel',
            payload: { sessionId },
          });
        }
        return { success: true };
      }
      case 'sidebar.send': {
        const chatMsg = args.message || args;
        const msgStr = typeof chatMsg === 'string' ? chatMsg : JSON.stringify(chatMsg);
        console.info(
          '[Canvas] Sending sidebar:sendMessage via bridge, msg:',
          msgStr.substring(0, 100)
        );
        const res = await NevofluxPage.sendQuery('bridge:request', {
          type: 'sidebar:sendMessage',
          payload: { message: msgStr },
        });
        return res;
      }
      case 'agent.sendCommand':
        return bridge.agent.sendCommand(args.command, args.params);
      case 'sidebar.open':
        return NevofluxPage.sendQuery('bridge:request', {
          type: 'sidebar:open',
          payload: {},
        });
      case 'sidebar.notify':
        return bridge.sidebar.notify(args.type, args.data);
      case 'storage.get': {
        const res = await bridge.storage.get(args.key);
        return res?.value ?? null;
      }
      case 'storage.set': {
        await bridge.storage.set(args.key, args.value);
        return true;
      }
      case 'storage.delete': {
        const res = await bridge.storage.delete(args.key);
        return res?.success ?? false;
      }
      case 'storage.query': {
        const res = await bridge.storage.query(args.prefix);
        return res?.results ?? [];
      }
      case 'system.getInfo':
        return bridge.system.getInfo();

      // ── Canvas Share / Import ────────────────────────────
      case 'canvas.share':
      case 'canvas.import':
      case 'canvas.share.extend':
      case 'canvas.share.delete':
      case 'canvas.share.list':
      // ── Canvas Tool Whitelist ────────────────────────────
      case 'canvas.tool.invoke':
      case 'canvas.tool.list':
      // ── EventBus ─────────────────────────────────────────
      case 'events.subscribe':
      case 'events.unsubscribe':
      case 'events.publish':
      case 'events.history':
      case 'events.recover': {
        // Forward to background.js which has the bridge handler.
        // The actor returns { success, data } — unwrap so the SDK
        // caller sees the raw response payload directly.
        const res = await NevofluxPage.sendQuery('bridge:request', {
          type: method,
          payload: args || {},
        });
        if (res && res.success === false) {
          throw new Error(res.error?.message || ('Bridge request failed: ' + method));
        }
        return res?.data !== undefined ? res.data : res;
      }

      default:
        throw new Error('Unknown method: ' + method);
    }
  },

  // ── Export & Share ─────────────────────────────────────

  async _exportHtml() {
    if (!this._artifact?.content) return;
    const type = this._artifact.type || 'html';
    let html;

    if (type === 'html' || type === 'svg') {
      html = this._artifact.content.trim().startsWith('<!DOCTYPE')
        ? this._artifact.content
        : `<!DOCTYPE html>\n<html>\n<head><meta charset="utf-8"><title>${this._escapeHtml(this._artifact.title || 'Artifact')}</title></head>\n<body>\n${this._artifact.content}\n</body>\n</html>`;
    } else if (type === 'react') {
      html = await this._buildReactStandaloneHtml(this._artifact);
    } else if (type === 'markdown') {
      html = await this._buildMarkdownStandaloneHtml(this._artifact);
    } else if (type === 'slides') {
      const slides = this._parseSlides(this._artifact.content);
      const slidesHtml = slides.map((s, i) => {
        const rendered = typeof markdownit !== 'undefined' ? markdownit({ html: true }).render(s) : this._escapeHtml(s);
        return `<section class="slide" id="slide-${i}">${rendered}</section>`;
      }).join('\n');
      html = `<!DOCTYPE html><html><head><meta charset="utf-8"><title>${this._escapeHtml(this._artifact.title || 'Presentation')}</title>
<style>
  body { font-family: system-ui, sans-serif; margin: 0; background: #1a1a2e; color: #e0e0e0; }
  .slide { min-height: 100vh; padding: 60px; display: flex; flex-direction: column; justify-content: center; box-sizing: border-box; border-bottom: 1px solid #333; }
  .slide h1, .slide h2, .slide h3 { margin-top: 0; }
  .slide ul, .slide ol { line-height: 1.8; }
  code { background: rgba(255,255,255,0.1); padding: 2px 6px; border-radius: 3px; }
  pre { background: rgba(0,0,0,0.3); padding: 16px; border-radius: 8px; overflow-x: auto; }
  table { border-collapse: collapse; width: 100%; } th, td { border: 1px solid #444; padding: 8px 12px; }
  img { max-width: 100%; }
</style></head><body>
${slidesHtml}
</body></html>`;
    } else {
      html = `<!DOCTYPE html>\n<html>\n<head><meta charset="utf-8"><title>${this._escapeHtml(this._artifact.title || 'Artifact')}</title></head>\n<body><pre>${this._escapeHtml(this._artifact.content)}</pre></body>\n</html>`;
    }

    this._downloadFile(html, `${this._artifact.title || 'artifact'}.html`, 'text/html');
  },

  _exportSource() {
    if (!this._artifact?.content) return;
    const extMap = {
      html: 'html',
      react: 'jsx',
      markdown: 'md',
      slides: 'md',
      svg: 'svg',
      mermaid: 'mmd',
      css: 'css',
      js: 'js',
    };
    const ext = extMap[this._artifact.type] || 'txt';
    const filename = `${this._artifact.title || 'artifact'}.${ext}`;
    const mime = ext === 'html' ? 'text/html' : ext === 'md' ? 'text/markdown' : 'text/plain';
    this._downloadFile(this._artifact.content, filename, mime);
  },

  async _exportImage() {
    await this._loadVendor('html2canvas.min.js');
    const html = this._lastContentHtml;
    if (!html) { this._showToast('Nothing to export', 'error'); return; }

    // Parse stored HTML to extract styles + body content
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');

    // Build a temporary container with styles and body in the parent document
    const container = document.createElement('div');
    container.style.cssText = 'position:absolute;left:-9999px;top:0;width:800px;';

    // Adopt style elements into parent document
    doc.querySelectorAll('style').forEach(s => {
      container.appendChild(document.adoptNode(s));
    });

    // Copy body content as innerHTML (avoids cross-document node issues)
    const bodyDiv = document.createElement('div');
    bodyDiv.innerHTML = doc.body.innerHTML;
    container.appendChild(bodyDiv);
    document.body.appendChild(container);

    try {
      const canvas = await html2canvas(container, { useCORS: true, scale: 2 });
      canvas.toBlob(blob => {
        if (blob) this._downloadBlob(blob, `${this._artifact.title || 'artifact'}.png`);
      }, 'image/png');
    } finally {
      document.body.removeChild(container);
    }
  },

  _exportPdf() {
    // Open clean content as a blob URL in a new tab for printing
    const html = this._lastContentHtml;
    if (!html) { this._showToast('Nothing to export', 'error'); return; }

    const blob = new Blob([html], { type: 'text/html;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.target = '_blank';
    a.click();
    // Clean up after user has had time to print
    setTimeout(() => URL.revokeObjectURL(url), 120000);
  },

  async _exportDocx() {
    await this._loadVendor('html-docx.min.js');
    // Use clean pre-injection HTML (no SDK scripts) for DOCX conversion
    const html = this._lastContentHtml;
    if (!html) { this._showToast('Nothing to export', 'error'); return; }

    const blob = htmlDocx.asBlob(html);
    this._downloadBlob(blob, `${this._artifact.title || 'artifact'}.docx`);
  },

  _exportSvg() {
    const iframeDoc = this._getPreviewDocument();
    if (!iframeDoc) { this._showToast('Nothing to export', 'error'); return; }

    const svg = iframeDoc.querySelector('svg');
    if (!svg) { this._showToast('No SVG found', 'error'); return; }

    const clone = svg.cloneNode(true);
    clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');

    const defs = iframeDoc.querySelectorAll('defs');
    defs.forEach(d => clone.prepend(d.cloneNode(true)));

    const svgString = new XMLSerializer().serializeToString(clone);
    this._downloadFile(svgString, `${this._artifact.title || 'artifact'}.svg`, 'image/svg+xml');
  },

  async _exportMarkdown() {
    await this._loadVendor('turndown.min.js');
    const iframeDoc = this._getPreviewDocument();
    if (!iframeDoc) { this._showToast('Nothing to export', 'error'); return; }

    const turndownService = new TurndownService();
    const md = turndownService.turndown(iframeDoc.body.innerHTML);
    this._downloadFile(md, `${this._artifact.title || 'artifact'}.md`, 'text/markdown');
  },

  async _exportPptx() {
    await this._loadVendor('jszip.min.js');
    await this._loadVendor('pptxgenjs.min.js');

    if (!this._artifact?.content) { this._showToast('Nothing to export', 'error'); return; }

    const slides = this._parseSlides(this._artifact.content);
    if (!slides.length) { this._showToast('No slides found', 'error'); return; }

    const pptx = new PptxGenJS();
    pptx.title = this._artifact.title || 'Presentation';
    pptx.layout = 'LAYOUT_WIDE';

    for (const slideMd of slides) {
      const slide = pptx.addSlide();
      const lines = slideMd.split('\n');
      let title = '';
      const bodyLines = [];

      for (const line of lines) {
        const headingMatch = line.match(/^#{1,3}\s+(.+)/);
        if (headingMatch && !title) {
          title = headingMatch[1].trim();
        } else if (line.trim()) {
          bodyLines.push(line.replace(/^[-*]\s+/, '• ').replace(/^\d+\.\s+/, ''));
        }
      }

      if (title) {
        slide.addText(title, {
          x: 0.5, y: 0.3, w: '90%', h: 1,
          fontSize: 28, bold: true, color: '363636',
        });
      }

      if (bodyLines.length) {
        slide.addText(
          bodyLines.map(t => ({ text: t, options: { bullet: t.startsWith('• '), breakLine: true } })),
          { x: 0.5, y: title ? 1.5 : 0.5, w: '90%', h: 4, fontSize: 16, color: '555555', valign: 'top' }
        );
      }
    }

    const blob = await pptx.write({ outputType: 'blob' });
    this._downloadBlob(blob, `${this._artifact.title || 'presentation'}.pptx`);
  },

  async _exportXlsx() {
    await this._loadVendor('xlsx.min.js');
    const iframeDoc = this._getPreviewDocument();
    if (!iframeDoc) { this._showToast('Nothing to export', 'error'); return; }

    const tables = iframeDoc.querySelectorAll('table');
    if (!tables.length) { this._showToast('No tables found', 'error'); return; }

    const wb = XLSX.utils.book_new();
    tables.forEach((table, i) => {
      const ws = XLSX.utils.table_to_sheet(table);
      XLSX.utils.book_append_sheet(wb, ws, `Sheet${i + 1}`);
    });
    const wbout = XLSX.write(wb, { bookType: 'xlsx', type: 'array' });
    const blob = new Blob([wbout], { type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' });
    this._downloadBlob(blob, `${this._artifact.title || 'artifact'}.xlsx`);
  },

  async _exportZip() {
    await this._loadVendor('jszip.min.js');
    if (!this._artifact?.files) { this._showToast('No project files found', 'error'); return; }

    const zip = new JSZip();
    for (const [path, content] of Object.entries(this._artifact.files)) {
      zip.file(path, content || '');
    }

    const blob = await zip.generateAsync({ type: 'blob' });
    this._downloadBlob(blob, `${this._artifact.title || 'project'}.zip`);
  },

  async _shareLink() {
    const url = `nevoflux://canvas/${this._artifactId}`;
    try {
      await navigator.clipboard.writeText(url);
      const btn = document.getElementById('btn-share');
      const originalSvg = btn.innerHTML;
      btn.innerHTML =
        '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>';
      btn.classList.add('success');
      setTimeout(() => {
        btn.innerHTML = originalSvg;
        btn.classList.remove('success');
      }, 1500);
    } catch (e) {
      console.error('Failed to copy link:', e);
    }
  },

  _downloadFile(content, filename, mimeType) {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  },

  _downloadBlob(blob, filename) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  },

  async _loadVendor(name) {
    if (this._vendorLoaded?.[name]) return;
    const script = document.createElement('script');
    script.src = `chrome://nevoflux/content/vendor/${name}`;
    await new Promise((resolve, reject) => {
      script.onload = resolve;
      script.onerror = () => reject(new Error(`Failed to load ${name}`));
      document.head.appendChild(script);
    });
    if (!this._vendorLoaded) this._vendorLoaded = {};
    this._vendorLoaded[name] = true;
  },

  async _safeExport(exportFn, formatName) {
    const btn = document.getElementById('btn-export-default');
    try {
      btn?.classList.add('exporting');
      await exportFn.call(this);
    } catch (err) {
      console.error(`Export ${formatName} failed:`, err);
      this._showToast(`Export failed: ${err.message}`, 'error');
    } finally {
      btn?.classList.remove('exporting');
    }
  },

  _showToast(message, type = 'info') {
    const existing = document.querySelector('.export-toast');
    if (existing) existing.remove();
    const toast = document.createElement('div');
    toast.className = `export-toast export-toast-${type}`;
    toast.textContent = message;
    toast.style.cssText = 'position:fixed;bottom:20px;left:50%;transform:translateX(-50%);padding:8px 16px;border-radius:6px;font-size:13px;z-index:9999;background:#333;color:#fff;';
    if (type === 'error') toast.style.background = '#c0392b';
    document.body.appendChild(toast);
    setTimeout(() => toast.remove(), 3000);
  },

  _updateConditionalFormats() {
    const type = this._artifact?.type || '';
    const iframeDoc = this._getPreviewDocument();
    const hasTables = iframeDoc ? iframeDoc.querySelectorAll('table').length > 0 : false;

    const conditions = {
      svg:      type === 'svg' || type === 'mermaid',
      markdown: type === 'html',
      pptx:     type === 'slides',
      xlsx:     hasTables,
      zip:      type === 'project',
    };

    let anyVisible = false;
    for (const [format, visible] of Object.entries(conditions)) {
      const el = document.querySelector(`[data-format="${format}"]`);
      if (el) {
        el.style.display = visible ? '' : 'none';
        if (visible) anyVisible = true;
      }
    }
    const sep = document.querySelector('.export-conditional-sep');
    if (sep) sep.style.display = anyVisible ? '' : 'none';

    // Update Source extension label dynamically
    const extMap = { html: '.html', react: '.jsx', markdown: '.md', svg: '.svg', mermaid: '.mmd', slides: '.md', css: '.css', js: '.js' };
    const extLabel = document.getElementById('export-ext-source');
    if (extLabel) extLabel.textContent = `(${extMap[type] || '.txt'})`;
  },

  async _buildReactStandaloneHtml(artifact) {
    const [react, reactDom, babel] = await Promise.all([
      this._fetchVendor('chrome://nevoflux/content/vendor/react.production.min.js'),
      this._fetchVendor('chrome://nevoflux/content/vendor/react-dom.production.min.js'),
      this._fetchVendor('chrome://nevoflux/content/vendor/babel.min.js'),
    ]);

    const title = this._escapeHtml(artifact.title || 'Artifact');
    return `<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>${title}</title>
<style>body{margin:0;font-family:system-ui,sans-serif;}</style>
<script>${react}</script>
<script>${reactDom}</script>
<script>${babel}</script>
</head><body>
<div id="root"></div>
<script type="text/babel" data-type="module">
${artifact.content}
const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(React.createElement(typeof App !== 'undefined' ? App : () => React.createElement('div', null, 'No App component found')));
</script>
</body></html>`;
  },

  async _buildMarkdownStandaloneHtml(artifact) {
    const [markdownIt, highlightJs] = await Promise.all([
      this._fetchVendor('chrome://nevoflux/content/vendor/markdown-it.min.js'),
      this._fetchVendor('chrome://nevoflux/content/vendor/highlight.min.js'),
    ]);

    const title = this._escapeHtml(artifact.title || 'Artifact');
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
<script>${markdownIt}</script>
<script>${highlightJs}</script>
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
</script>
</body></html>`;
  },

  // ── Data Loading ────────────────────────────────────────

  async _loadArtifact() {
    try {
      const result = await NevofluxPage.sendQuery('contentStore:get', {
        key: `canvas:${this._artifactId}`,
      });

      if (result && result.value) {
        this._artifact = result.value;
        this._onArtifactUpdate(result.value);
      } else {
        // Not in ContentStore — request hydration from backend (spinner already visible from HTML default)
        this._updateStatus('Loading');

        NevofluxPage.sendQuery('bridge:request', {
          type: 'send_to_agent',
          payload: {
            type: 'system_command',
            payload: {
              request_id: `art-get-${Date.now()}`,
              command: 'artifact.get',
              params: { artifact_id: this._artifactId },
            },
          },
        }).catch((e) => console.warn('[Canvas] artifact.get request failed:', e));

        this._loadTimeout = setTimeout(() => {
          if (!this._artifact) {
            this._showEmpty('Artifact not found');
            this._updateStatus('Not found');
          }
        }, 5000);
      }

      // Subscribe to future updates (clears loading state when data arrives)
      NevofluxPage.sendMessage('contentStore:subscribe', {
        key: `canvas:${this._artifactId}`,
      });
    } catch (e) {
      console.error('[Canvas] Failed to load artifact:', e);
      this._showEmpty('Failed to load artifact');
    }
  },

  _onArtifactUpdate(artifact) {
    if (this._loadTimeout) {
      clearTimeout(this._loadTimeout);
      this._loadTimeout = null;
    }

    if (!artifact) {
      this._showEmpty('Artifact deleted');
      return;
    }

    this._artifact = artifact;
    this._hideLoading();

    // Update toolbar
    document.getElementById('artifact-title').textContent = artifact.title || 'Untitled';
    document.title = artifact.title || 'Canvas';

    const stateLabels = {
      streaming: 'Generating...',
      complete: 'Ready',
      error: 'Error',
    };
    this._updateStatus(stateLabels[artifact.state] || artifact.state);

    // Debounce rendering (300ms for streaming, immediate for complete)
    if (this._debounceTimer) {
      clearTimeout(this._debounceTimer);
    }

    if (artifact.state === 'complete') {
      this._render(artifact);
    } else {
      this._debounceTimer = setTimeout(() => {
        this._render(artifact);
      }, 300);
    }
  },

  // ── Rendering ───────────────────────────────────────────

  async _render(artifact) {
    this._hideLoading();

    // Clean up slides keyboard handler on artifact change
    if (this._slidesKeyHandler) {
      document.removeEventListener('keydown', this._slidesKeyHandler);
      this._slidesKeyHandler = null;
    }

    console.error(
      `[Canvas] _render: type=${artifact.type}, contentLen=${artifact.content?.length}, state=${artifact.state}, mode=${this._mode}`
    );

    if (this._mode === 'edit') {
      this._updateEditPreview(artifact);
      return;
    }

    const viewport = document.getElementById('viewport');
    const emptyState = document.getElementById('empty-state');

    if (!artifact.content && !artifact.files) {
      console.error(`[Canvas] _render: content and files are both empty/falsy`);
      this._showEmpty('Empty artifact');
      return;
    }

    // Hide empty state
    if (emptyState) {
      emptyState.style.display = 'none';
    }

    // Normalize type (handle both short types and MIME types)
    const type = this._normalizeType(artifact.type);
    console.error(`[Canvas] _render: normalizedType=${type}`);
    switch (type) {
      case 'html':
        // Auto-detect React HTML: if HTML contains React CDN scripts or <script type="text/babel">,
        // extract JSX code and render via _renderReact which inlines vendor scripts.
        if (this._isReactHtml(artifact.content)) {
          console.info('[Canvas] Auto-detected React in HTML content, using _renderReact');
          const jsx = this._extractReactCode(artifact.content);
          this._renderReact(viewport, jsx);
        } else {
          this._renderSrcdoc(viewport, artifact.content);
        }
        break;
      case 'react':
        this._renderReact(viewport, artifact.content);
        break;
      case 'markdown':
        this._renderMarkdown(viewport, artifact.content);
        break;
      case 'svg':
        this._renderSrcdoc(viewport, artifact.content);
        break;
      case 'mermaid':
        this._renderMermaid(viewport, artifact.content);
        break;
      case 'slides':
        await this._renderSlides(viewport);
        break;
      case 'project':
        this._renderProject(viewport, artifact);
        break;
      default:
        this._showEmpty(`Unsupported type: ${artifact.type}`);
    }
  },

  _parseSlides(markdownContent) {
    return markdownContent
      .split(/\r?\n---+\s*\r?\n/)
      .map(slide => slide.trim())
      .filter(slide => slide.length > 0);
  },

  async _renderSlides(viewport) {
    const slides = this._parseSlides(this._artifact.content || '');
    if (!slides.length) return;

    // Clean up previous slides keyboard handler
    if (this._slidesKeyHandler) {
      document.removeEventListener('keydown', this._slidesKeyHandler);
      this._slidesKeyHandler = null;
    }

    this._currentSlideIndex = 0;
    this._slidesData = slides;
    viewport.innerHTML = '';

    // Load markdown-it for rendering (registers global `markdownit`)
    await this._loadVendor('markdown-it.min.js');
    const md = markdownit({ html: true, linkify: true });

    const container = document.createElement('div');
    container.className = 'slides-container';

    // Left panel: thumbnails
    const thumbPanel = document.createElement('div');
    thumbPanel.className = 'slides-thumb-panel';

    // Right panel: main preview
    const mainPanel = document.createElement('div');
    mainPanel.className = 'slides-main-panel';

    const pageIndicator = document.createElement('div');
    pageIndicator.className = 'slides-page-indicator';
    mainPanel.appendChild(pageIndicator);

    // Render each slide thumbnail
    slides.forEach((slideMd, i) => {
      const thumb = document.createElement('div');
      thumb.className = 'slides-thumb' + (i === 0 ? ' active' : '');
      thumb.innerHTML = `<div class="slides-thumb-number">${i + 1}</div><div class="slides-thumb-preview">${md.render(slideMd)}</div>`;
      thumb.addEventListener('click', () => this._selectSlide(i, thumbPanel, mainPanel, pageIndicator, md));
      thumbPanel.appendChild(thumb);
    });

    container.appendChild(thumbPanel);
    container.appendChild(mainPanel);
    viewport.appendChild(container);

    // Render first slide
    this._selectSlide(0, thumbPanel, mainPanel, pageIndicator, md);

    // Keyboard navigation for slides
    this._slidesKeyHandler = (e) => {
      if (document.querySelector('.export-dropdown.open')) return;
      if (e.key === 'ArrowDown' || e.key === 'ArrowRight') {
        e.preventDefault();
        const next = Math.min(this._currentSlideIndex + 1, slides.length - 1);
        this._selectSlide(next, thumbPanel, mainPanel, pageIndicator, md);
      } else if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') {
        e.preventDefault();
        const prev = Math.max(this._currentSlideIndex - 1, 0);
        this._selectSlide(prev, thumbPanel, mainPanel, pageIndicator, md);
      }
    };
    document.addEventListener('keydown', this._slidesKeyHandler);
  },

  _selectSlide(index, thumbPanel, mainPanel, pageIndicator, md) {
    this._currentSlideIndex = index;
    const slides = this._slidesData;

    // Update thumbnails
    thumbPanel.querySelectorAll('.slides-thumb').forEach((t, i) => {
      t.classList.toggle('active', i === index);
    });
    const activeThumb = thumbPanel.querySelector('.slides-thumb.active');
    if (activeThumb) activeThumb.scrollIntoView({ behavior: 'smooth', block: 'nearest' });

    // Render selected slide in a DEDICATED iframe (not this._iframe)
    const slideHtml = md.render(slides[index]);
    const fullHtml = `<!DOCTYPE html><html><head><meta charset="utf-8"><style>
    body { font-family: system-ui, sans-serif; padding: 40px; margin: 0; color: #e0e0e0; background: #1a1a2e; display: flex; flex-direction: column; justify-content: center; min-height: calc(100vh - 80px); }
    h1, h2, h3 { margin-top: 0; } ul, ol { padding-left: 24px; line-height: 1.8; }
    code { background: rgba(255,255,255,0.1); padding: 2px 6px; border-radius: 3px; }
    pre { background: rgba(0,0,0,0.3); padding: 16px; border-radius: 8px; overflow-x: auto; }
    table { border-collapse: collapse; width: 100%; } th, td { border: 1px solid #444; padding: 8px 12px; text-align: left; }
    th { background: rgba(255,255,255,0.05); } img { max-width: 100%; border-radius: 8px; }
  </style></head><body>${slideHtml}</body></html>`;

    // Use a dedicated iframe, separate from this._iframe
    let slideIframe = mainPanel.querySelector('iframe');
    if (!slideIframe) {
      slideIframe = document.createElement('iframe');
      slideIframe.sandbox = 'allow-same-origin';
      slideIframe.style.cssText = 'flex:1;border:none;width:100%;';
      mainPanel.appendChild(slideIframe);
    }
    slideIframe.srcdoc = fullHtml;

    pageIndicator.textContent = `${index + 1} / ${slides.length}`;
  },

  /**
   * Render raw HTML/SVG content in a sandboxed iframe.
   * Injects NevofluxSDK postMessage bridge into every srcdoc.
   */
  _renderSrcdoc(viewport, htmlContent) {
    if (this._iframe) {
      this._iframe.remove();
    }
    this._iframe = document.createElement('iframe');
    this._iframe.setAttribute('sandbox', 'allow-scripts allow-forms allow-same-origin');

    // Inject NevofluxSDK into srcdoc using indexOf+slice (avoid String.replace
    // which can interpret $-patterns in the replacement string).
    let injected = htmlContent;
    const sdk = this._SDK_SCRIPT;
    let idx = injected.indexOf('</head>');
    if (idx !== -1) {
      injected = injected.slice(0, idx) + sdk + injected.slice(idx);
    } else {
      idx = injected.indexOf('<body');
      if (idx !== -1) {
        injected = injected.slice(0, idx) + sdk + injected.slice(idx);
      } else {
        injected = sdk + injected;
      }
    }

    console.error(
      `[Canvas] _renderSrcdoc: inputLen=${htmlContent.length}, injectedLen=${injected.length}, sdkLen=${sdk.length}`
    );
    this._iframe.srcdoc = injected;
    this._lastRenderedHtml = injected;
    this._lastContentHtml = htmlContent; // pre-injection, clean HTML for export
    viewport.appendChild(this._iframe);
  },

  /**
   * Render React JSX code. Fetches vendor libs from chrome:// and inlines them
   * into srcdoc since the sandbox has no allow-same-origin.
   */
  async _renderReact(viewport, code) {
    const [react, reactDom, babel] = await Promise.all([
      this._fetchVendor('chrome://nevoflux/content/vendor/react.production.min.js'),
      this._fetchVendor('chrome://nevoflux/content/vendor/react-dom.production.min.js'),
      this._fetchVendor('chrome://nevoflux/content/vendor/babel.min.js'),
    ]);

    const srcdoc = `<!DOCTYPE html>
<html><head><meta charset="utf-8">
<style>body{margin:0;font-family:system-ui,sans-serif;}</style>
<script>${react}</script>
<script>${reactDom}</script>
<script>${babel}</script>
</head><body>
<div id="root"></div>
<script type="text/babel" data-type="module">
${code}
const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(React.createElement(typeof App !== 'undefined' ? App : () => React.createElement('div', null, 'No App component found')));
</script>
</body></html>`;

    this._renderSrcdoc(viewport, srcdoc);
  },

  /**
   * Render Markdown content using markdown-it.
   */
  async _renderMarkdown(viewport, markdown) {
    const markdownIt = await this._fetchVendor(
      'chrome://nevoflux/content/vendor/markdown-it.min.js'
    );
    const highlightJs = await this._fetchVendor(
      'chrome://nevoflux/content/vendor/highlight.min.js'
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
<script>${markdownIt}</script>
<script>${highlightJs}</script>
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
</script>
</body></html>`;

    this._renderSrcdoc(viewport, srcdoc);
  },

  /**
   * Render Mermaid diagrams.
   */
  async _renderMermaid(viewport, code) {
    const mermaid = await this._fetchVendor('chrome://nevoflux/content/vendor/mermaid.min.js');

    const srcdoc = `<!DOCTYPE html>
<html><head><meta charset="utf-8">
<style>body{margin:24px;display:flex;justify-content:center;}</style>
<script>${mermaid}</script>
</head><body>
<div class="mermaid">${this._escapeHtml(code)}</div>
<script>mermaid.initialize({ startOnLoad: true, theme: 'default' });</script>
</body></html>`;

    this._renderSrcdoc(viewport, srcdoc);
  },

  /**
   * Render a multi-file project using CanvasRuntime.
   * @param {HTMLElement} viewport
   * @param {object} artifact - Must have .files field
   */
  async _renderProject(viewport, artifact) {
    if (!artifact.files || Object.keys(artifact.files).length === 0) {
      this._showEmpty('No files in project');
      return;
    }

    this._updateStatus('Bundling...');

    const result = await CanvasRuntime.render(
      viewport,
      {
        files: artifact.files,
        entry: artifact.entry,
        options: artifact.options,
      },
      this._SDK_SCRIPT
    );

    if (result.success) {
      this._iframe = CanvasRuntime._iframe;
      this._updateStatus('Ready');
    } else {
      this._showEmpty(`Build error: ${result.error}`);
      this._updateStatus('Error');
    }
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
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  },

  _getPreviewDocument() {
    // Try iframe contentDocument first (may be null due to sandbox/origin restrictions)
    if (this._iframe?.contentDocument) {
      return this._iframe.contentDocument;
    }
    // Fallback: parse the saved rendered HTML into a temporary document
    if (this._lastRenderedHtml) {
      const parser = new DOMParser();
      return parser.parseFromString(this._lastRenderedHtml, 'text/html');
    }
    return null;
  },

  // ── Edit Mode ───────────────────────────────────────────

  _enterEditMode() {
    // Multi-file project: use project-specific edit mode
    if (this._artifact?.files) {
      this._enterProjectEditMode();
      return;
    }

    const viewport = document.getElementById('viewport');
    const emptyState = document.getElementById('empty-state');
    if (emptyState) emptyState.style.display = 'none';
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
    viewport.innerHTML = '';
    const split = document.createElement('div');
    split.className = 'canvas-split';
    split.id = 'edit-split';

    const editorPane = document.createElement('div');
    editorPane.className = 'editor-pane';
    editorPane.id = 'code-editor-pane';

    const previewPane = document.createElement('div');
    previewPane.className = 'preview-pane';
    previewPane.id = 'edit-preview';

    split.appendChild(editorPane);
    split.appendChild(previewPane);
    viewport.appendChild(split);

    // Detect dark theme
    const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;

    // Live preview debounce
    let editDebounce = null;

    // Create CodeMirror editor (falls back to textarea if bundle not loaded)
    if (window.CodeMirrorFactory) {
      this._cmView = window.CodeMirrorFactory.create(editorPane, {
        value: this._artifact?.content || '',
        language: this._artifact?.type || 'html',
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
      const textarea = document.createElement('textarea');
      textarea.id = 'code-editor';
      textarea.value = this._artifact?.content || '';
      textarea.spellcheck = false;
      textarea.addEventListener('keydown', (e) => {
        if (e.key === 'Tab') {
          e.preventDefault();
          const start = textarea.selectionStart;
          const end = textarea.selectionEnd;
          textarea.value =
            textarea.value.substring(0, start) + '  ' + textarea.value.substring(end);
          textarea.selectionStart = textarea.selectionEnd = start + 2;
        }
      });
      textarea.addEventListener('input', () => {
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

  _enterProjectEditMode() {
    const viewport = document.getElementById('viewport');
    viewport.innerHTML = '';

    // Create split layout
    const container = document.createElement('div');
    container.className = 'canvas-split';
    container.id = 'edit-split';

    // File selector bar
    const selectorBar = document.createElement('div');
    selectorBar.style.cssText =
      'padding:8px;border-bottom:1px solid var(--zen-colors-border, #333);display:flex;align-items:center;gap:8px;';

    const label = document.createElement('span');
    label.textContent = 'File:';
    label.style.cssText = 'font-size:12px;color:var(--zen-colors-secondary, #999);';
    selectorBar.appendChild(label);

    const select = document.createElement('select');
    select.id = 'project-file-select';
    select.style.cssText =
      'flex:1;padding:4px 8px;background:var(--zen-colors-input-bg, #1a1a1a);color:inherit;border:1px solid var(--zen-colors-border, #333);border-radius:4px;font-size:12px;font-family:monospace;';
    const files = Object.keys(this._artifact.files).sort();
    for (const path of files) {
      const opt = document.createElement('option');
      opt.value = path;
      opt.textContent = path;
      select.appendChild(opt);
    }
    selectorBar.appendChild(select);

    // Editor pane
    const editorPane = document.createElement('div');
    editorPane.className = 'editor-pane';
    editorPane.id = 'code-editor-pane';

    // Preview pane
    const previewPane = document.createElement('div');
    previewPane.className = 'preview-pane';
    previewPane.id = 'edit-preview';

    container.appendChild(editorPane);
    container.appendChild(previewPane);
    viewport.appendChild(selectorBar);
    viewport.appendChild(container);

    let currentPath = files[0];

    // Initialize editor
    const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const initialContent = this._artifact.files[currentPath] || '';

    let debounceTimer = null;
    const onEdit = (value) => {
      this._artifact.files[currentPath] = value;
      clearTimeout(debounceTimer);
      debounceTimer = setTimeout(async () => {
        await CanvasRuntime.render(
          previewPane,
          { files: this._artifact.files, entry: this._artifact.entry },
          this._SDK_SCRIPT
        );
      }, 500);
    };

    const loadFile = (path) => {
      // Save current file content first
      if (this._cmView && window.CodeMirrorFactory) {
        this._artifact.files[currentPath] = window.CodeMirrorFactory.getValue(this._cmView);
      }
      currentPath = path;
      const content = this._artifact.files[path] || '';
      if (this._cmView && window.CodeMirrorFactory) {
        window.CodeMirrorFactory.setValue(this._cmView, content);
      } else {
        const ta = document.getElementById('code-editor');
        if (ta) ta.value = content;
      }
    };

    select.addEventListener('change', () => loadFile(select.value));

    if (window.CodeMirrorFactory) {
      this._cmView = window.CodeMirrorFactory.create(editorPane, {
        value: initialContent,
        language: currentPath.split('.').pop() || 'html',
        dark: isDark,
        onChange: onEdit,
      });
    } else {
      const textarea = document.createElement('textarea');
      textarea.id = 'code-editor';
      textarea.value = initialContent;
      textarea.style.cssText =
        'width:100%;height:100%;resize:none;background:#1a1a1a;color:#d4d4d4;border:none;padding:12px;font-family:monospace;font-size:13px;tab-size:2;';
      textarea.addEventListener('input', () => onEdit(textarea.value));
      textarea.addEventListener('keydown', (e) => {
        if (e.key === 'Tab') {
          e.preventDefault();
          const start = textarea.selectionStart;
          textarea.value =
            textarea.value.substring(0, start) +
            '  ' +
            textarea.value.substring(textarea.selectionEnd);
          textarea.selectionStart = textarea.selectionEnd = start + 2;
          onEdit(textarea.value);
        }
      });
      editorPane.appendChild(textarea);
    }

    // Initial preview render
    CanvasRuntime.render(
      previewPane,
      { files: this._artifact.files, entry: this._artifact.entry },
      this._SDK_SCRIPT
    );
  },

  _exitEditMode() {
    // Save project files if in project edit mode
    if (this._artifact?.files) {
      const select = document.getElementById('project-file-select');
      if (select) {
        // Save current file content
        if (this._cmView && window.CodeMirrorFactory) {
          this._artifact.files[select.value] = window.CodeMirrorFactory.getValue(this._cmView);
        } else {
          const ta = document.getElementById('code-editor');
          if (ta) this._artifact.files[select.value] = ta.value;
        }
        // Persist to ContentStore
        NevofluxPage.sendMessage('contentStore:set', {
          key: `canvas:${this._artifactId}`,
          value: this._artifact,
        });
      }
    }

    const split = document.getElementById('edit-split');
    if (split) {
      // Save edits back to ContentStore
      let newContent = null;
      if (this._cmView) {
        newContent = window.CodeMirrorFactory.getValue(this._cmView);
      } else {
        const editor = document.getElementById('code-editor');
        if (editor) newContent = editor.value;
      }

      if (newContent != null && this._artifact && newContent !== this._artifact.content) {
        this._artifact.content = newContent;
        NevofluxPage.sendMessage('contentStore:set', {
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
    const previewPane = document.getElementById('edit-preview');
    if (!previewPane || !this._artifact) return;

    // Create a temporary artifact for rendering
    const tempArtifact = { ...this._artifact, content: code };

    // Remove old iframe in preview pane
    const oldIframe = previewPane.querySelector('iframe');
    if (oldIframe) oldIframe.remove();

    // Render into preview pane
    switch (tempArtifact.type) {
      case 'html':
      case 'svg': {
        const iframe = document.createElement('iframe');
        iframe.setAttribute('sandbox', 'allow-scripts allow-forms allow-same-origin');
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
    const iframe = document.createElement('iframe');
    iframe.setAttribute('sandbox', 'allow-scripts allow-forms allow-same-origin');
    iframe.srcdoc = artifact.content;
    container.appendChild(iframe);
  },

  // ── Loading / Empty State ───────────────────────────────

  _showLoading() {
    const loading = document.getElementById('loading-state');
    const empty = document.getElementById('empty-state');
    if (loading) loading.style.display = 'flex';
    if (empty) empty.style.display = 'none';
    if (this._iframe) {
      this._iframe.remove();
      this._iframe = null;
    }
  },

  _hideLoading() {
    const loading = document.getElementById('loading-state');
    if (loading) loading.style.display = 'none';
  },

  _showEmpty(message) {
    this._hideLoading();
    const emptyState = document.getElementById('empty-state');
    if (emptyState) {
      emptyState.textContent = message;
      emptyState.style.display = 'flex';
    }
    if (this._iframe) {
      this._iframe.remove();
      this._iframe = null;
    }
  },

  _updateStatus(text) {
    document.getElementById('artifact-status').textContent = text;
  },

  /**
   * Normalize MIME type to short canvas renderer type.
   */
  _normalizeType(rawType) {
    if (!rawType) return 'html';
    const MIME_MAP = {
      'text/html': 'html',
      'text/markdown': 'markdown',
      'text/svg+xml': 'svg',
      'image/svg+xml': 'svg',
      'application/javascript': 'react',
      'text/jsx': 'react',
    };
    return MIME_MAP[rawType] || rawType;
  },

  /**
   * Detect if HTML content is actually a React app loaded via CDN scripts.
   * These won't work in sandboxed iframes, so we need to use _renderReact instead.
   */
  _isReactHtml(html) {
    return (
      /text\/babel/i.test(html) ||
      (/react/i.test(html) && /ReactDOM/i.test(html) && /createRoot|render/i.test(html))
    );
  },

  /**
   * Extract JSX/React code from an HTML page that loads React via CDN.
   * Returns just the JSX code for use with _renderReact().
   */
  _extractReactCode(html) {
    // Extract content from <script type="text/babel"> blocks
    const babelMatch = html.match(
      /<script[^>]*type\s*=\s*["']text\/babel["'][^>]*>([\s\S]*?)<\/script>/i
    );
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
window.addEventListener('NevofluxMessage', (event) => {
  const detail = event.detail;
  const { type, key, value } = detail;

  // ContentStore updates (existing)
  if (type === 'contentStore:update' && key === `canvas:${Canvas._artifactId}`) {
    console.error(
      `[Canvas] ContentStore update received: contentLen=${value?.content?.length}, state=${value?.state}`
    );
    Canvas._onArtifactUpdate(value);
    return;
  }

  // Agent push messages → forward to artifact iframe
  if (detail.sessionId && detail.message && Canvas._iframe?.contentWindow) {
    Canvas._iframe.contentWindow.postMessage(
      {
        _nevoflux: true,
        _agentPush: true,
        sessionId: detail.sessionId,
        message: detail.message,
      },
      '*'
    );
  }

  if (type === 'bridge:push') {
    if (detail.msg && Canvas._iframe?.contentWindow) {
      // SDK message listener gates on `_nevoflux: true` — preserve that
      // marker so canvas:tool:event / events:delivery dispatchers fire.
      const msg = detail.msg;
      const tagged = msg && typeof msg === 'object' ? { _nevoflux: true, ...msg } : msg;
      Canvas._iframe.contentWindow.postMessage(tagged, '*');
    }
  }
});

document.addEventListener('DOMContentLoaded', () => Canvas.init());

// =============================
// Share Dialog + Import Dialog + Canvas List
// =============================

const ShareDialog = {
  open() {
    this._show('share');
    this._showStep('share', 'confirm');
  },

  async confirm(artifactId) {
    this._showStep('share', 'loading');
    try {
      const result = await NevofluxSDK.share.share(artifactId);
      document.getElementById('share-url-input').value = result.share_url || '';
      document.getElementById('share-password-input').value = result.password || '';
      const expiresEl = document.getElementById('share-expires-date');
      if (result.expires_at) {
        expiresEl.textContent = new Date(result.expires_at * 1000).toLocaleDateString();
      } else {
        expiresEl.textContent = '';
      }
      this._showStep('share', 'result');
    } catch (err) {
      document.getElementById('share-error-msg').textContent =
        (err && err.message) || String(err);
      this._showStep('share', 'error');
    }
  },

  copyUrl() {
    const input = document.getElementById('share-url-input');
    navigator.clipboard.writeText(input.value).catch(() => {});
  },

  copyPassword() {
    const input = document.getElementById('share-password-input');
    const password = input.value;
    navigator.clipboard.writeText(password).then(() => {
      const notice = document.getElementById('share-clipboard-notice');
      const countdown = document.getElementById('share-clear-countdown');
      notice.hidden = false;
      let remaining = 60;
      countdown.textContent = remaining;
      if (this._clearInterval) clearInterval(this._clearInterval);
      this._clearInterval = setInterval(() => {
        remaining--;
        countdown.textContent = remaining;
        if (remaining <= 0) {
          clearInterval(this._clearInterval);
          this._clearInterval = null;
          navigator.clipboard.writeText('').catch(() => {});
          notice.hidden = true;
        }
      }, 1000);
    }).catch(() => {});
  },

  _show(which) {
    const el = document.getElementById(`nevoflux-${which}-dialog`);
    if (el) el.hidden = false;
  },

  _close(which) {
    const el = document.getElementById(`nevoflux-${which}-dialog`);
    if (el) el.hidden = true;
  },

  _showStep(dialog, step) {
    const steps = ['confirm', 'loading', 'result', 'error', 'prompt', 'success'];
    steps.forEach((s) => {
      const el = document.getElementById(`${dialog}-step-${s}`);
      if (el) el.hidden = (s !== step);
    });
  },
};

const ImportDialog = {
  _shareId: null,

  open(shareId) {
    this._shareId = shareId;
    document.getElementById('import-share-id').textContent = shareId || '';
    document.getElementById('import-password-input').value = '';
    ShareDialog._show('import');
    ShareDialog._showStep('import', 'prompt');
  },

  async submit() {
    const password = document.getElementById('import-password-input').value.trim();
    if (!password) return;

    ShareDialog._showStep('import', 'loading');
    try {
      const result = await NevofluxSDK.share.import(this._shareId, password);
      document.getElementById('import-result-name').textContent =
        (result && (result.artifact_name || result.artifact_id)) || '(imported)';
      ShareDialog._showStep('import', 'success');
    } catch (err) {
      document.getElementById('import-error-msg').textContent =
        (err && err.message) || String(err);
      ShareDialog._showStep('import', 'error');
    }
  },

  retry() {
    ShareDialog._showStep('import', 'prompt');
  },
};

const CanvasList = {
  _filter: 'all',
  _shares: [],

  async load() {
    try {
      const res = await NevofluxSDK.share.list();
      this._shares = (res && res.shares) || [];
      this._render();
    } catch (err) {
      console.error('[CanvasList] Failed to load:', err);
      this._shares = [];
      this._render();
    }
  },

  setFilter(filter) {
    this._filter = filter;
    document.querySelectorAll('.canvas-list-filters .filter-btn').forEach((btn) => {
      btn.classList.toggle('active', btn.dataset.filter === filter);
    });
    this._render();
  },

  _render() {
    const container = document.getElementById('canvas-list-items');
    const empty = document.getElementById('canvas-list-empty');
    if (!container || !empty) return;

    // Basic filter: 'shared' = all share entries; 'imported' = items flagged imported;
    // 'all' = everything. Artifact-only entries not included (no artifact SDK list here).
    let items = this._shares;
    if (this._filter === 'shared') {
      items = items.filter((s) => !s.imported);
    } else if (this._filter === 'imported') {
      items = items.filter((s) => !!s.imported);
    }

    if (!items || items.length === 0) {
      container.innerHTML = '';
      empty.hidden = false;
      return;
    }
    empty.hidden = true;

    const esc = (v) => String(v == null ? '' : v).replace(/[&<>"']/g, (c) =>
      ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])
    );

    container.innerHTML = items.map((s) => {
      const expires = s.expires_at
        ? new Date(s.expires_at * 1000).toLocaleDateString()
        : '—';
      const views = s.view_count != null ? s.view_count : 0;
      const name = s.artifact_name || s.artifact_id || s.share_id;
      const badgeClass = s.imported ? 'status-badge imported' : 'status-badge shared';
      const badgeLabel = s.imported ? '📥 Imported' : '🔗 Shared';
      return `
        <div class="canvas-list-item" data-share-id="${esc(s.share_id)}">
          <div class="canvas-item-name">${esc(name)}</div>
          <div class="canvas-item-meta">
            <span class="${badgeClass}">${badgeLabel}</span>
            <span>${esc(views)} views</span>
            <span>Expires: ${esc(expires)}</span>
          </div>
        </div>
      `;
    }).join('');
  },
};

document.addEventListener('DOMContentLoaded', () => {
  // Close buttons for modals
  document.querySelectorAll('[data-close-dialog]').forEach((el) => {
    el.addEventListener('click', () => {
      const dialog = el.closest('.nevoflux-modal');
      if (dialog) dialog.hidden = true;
    });
  });

  // Share dialog buttons
  const shareConfirmBtn = document.getElementById('share-confirm-btn');
  if (shareConfirmBtn) {
    shareConfirmBtn.addEventListener('click', () => {
      const artifactId =
        (Canvas && Canvas._artifactId) || window._nevofluxArtifactId;
      if (artifactId) ShareDialog.confirm(artifactId);
    });
  }

  const shareCopyUrl = document.getElementById('share-copy-url-btn');
  if (shareCopyUrl) shareCopyUrl.addEventListener('click', () => ShareDialog.copyUrl());

  const shareCopyPw = document.getElementById('share-copy-password-btn');
  if (shareCopyPw) shareCopyPw.addEventListener('click', () => ShareDialog.copyPassword());

  // Import dialog buttons
  const importSubmit = document.getElementById('import-submit-btn');
  if (importSubmit) importSubmit.addEventListener('click', () => ImportDialog.submit());

  const importRetry = document.getElementById('import-retry-btn');
  if (importRetry) importRetry.addEventListener('click', () => ImportDialog.retry());

  // Enter submits the import password
  const importPwInput = document.getElementById('import-password-input');
  if (importPwInput) {
    importPwInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        ImportDialog.submit();
      }
    });
  }

  // Canvas list: filter buttons
  document.querySelectorAll('.canvas-list-filters .filter-btn').forEach((btn) => {
    btn.addEventListener('click', () => CanvasList.setFilter(btn.dataset.filter));
  });

  // Decide which UI to show:
  // 1. If URL is import mode -> show import dialog automatically
  // 2. If no artifact id AND mode=list (or no id at all) -> show canvas list view
  if (window._nevofluxImportShareId) {
    ImportDialog.open(window._nevofluxImportShareId);
  }

  try {
    const url = new URL(window.location.href);
    const mode = (typeof NevofluxPage !== 'undefined' && NevofluxPage.getParam)
      ? NevofluxPage.getParam('mode', '')
      : (url.searchParams.get('mode') || '');
    const hasArtifactId = !!(Canvas && Canvas._artifactId);
    if (mode === 'list' || !hasArtifactId) {
      const listEl = document.getElementById('nevoflux-canvas-list');
      if (listEl) {
        listEl.hidden = false;
        CanvasList.load();
      }
    }
  } catch (e) {
    // ignore; list view is a progressive enhancement
  }
});

