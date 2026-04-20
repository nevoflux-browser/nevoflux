/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

const lazy = {};
ChromeUtils.defineESModuleGetters(lazy, {
  NevofluxContentStore: 'resource:///modules/NevofluxContentStore.sys.mjs',
  NevofluxBridgeRouter: 'resource:///modules/NevofluxBridgeRouter.sys.mjs',
});

export class NevofluxParent extends JSWindowActorParent {
  // Track pending dialog for this browsing context
  static _pendingDialogs = new WeakMap();

  constructor() {
    super();
    this._dialogObserver = null;
  }

  actorCreated() {
    // Register dialog observer when actor is created
    this._setupDialogObserver();
  }

  didDestroy() {
    // Cleanup observer when actor is destroyed
    this._removeDialogObserver();

    // Cleanup ContentStore subscriptions
    if (this._contentStoreSubscriptions) {
      for (const unsub of this._contentStoreSubscriptions) {
        try {
          unsub();
        } catch (e) {}
      }
      this._contentStoreSubscriptions = null;
    }

    // Cleanup agent session subscriptions
    if (this._agentSessionUnsubs) {
      for (const unsub of this._agentSessionUnsubs.values()) {
        try {
          unsub();
        } catch (e) {}
      }
      this._agentSessionUnsubs = null;
    }

    // Cleanup EventBus push channels
    if (this._eventChannelUnsubs) {
      for (const unsub of this._eventChannelUnsubs.values()) {
        try {
          unsub();
        } catch (e) {}
      }
      this._eventChannelUnsubs = null;
    }

    // Drop any canvas_video job registrations tied to this actor.
    this._canvasVideoJobs = null;
  }

  _setupDialogObserver() {
    if (this._dialogObserver) return;

    this._dialogObserver = {
      observe: (subject, topic, _data) => {
        if (topic === 'common-dialog-loaded') {
          // Store dialog reference for this window
          const dominated = subject.opener;
          if (dominated) {
            NevofluxParent._pendingDialogs.set(dominated, subject);
          }
        }
      },
    };

    try {
      Services.obs.addObserver(this._dialogObserver, 'common-dialog-loaded');
    } catch (e) {
      // Observer already added or Services not available
    }
  }

  _removeDialogObserver() {
    if (this._dialogObserver) {
      try {
        Services.obs.removeObserver(this._dialogObserver, 'common-dialog-loaded');
      } catch (e) {
        // Observer already removed
      }
      this._dialogObserver = null;
    }
  }

  async receiveMessage({ name, data }) {
    switch (name) {
      case 'dialogAccept':
        return this.acceptDialog(data?.text);
      case 'dialogDismiss':
        return this.dismissDialog();

      case 'canvasVideo:registerJob': {
        const jobId = data?.job_id;
        if (!jobId || typeof jobId !== 'string') {
          return { ok: false, error: 'job_id required' };
        }
        if (!this._canvasVideoJobs) {
          this._canvasVideoJobs = new Map();
        }
        if (!this._canvasVideoJobs.has(jobId)) {
          this._canvasVideoJobs.set(jobId, { created_at: Date.now() });
        }
        return { ok: true };
      }

      case 'canvasVideo:unregisterJob': {
        const jobId = data?.job_id;
        if (this._canvasVideoJobs && jobId) {
          this._canvasVideoJobs.delete(jobId);
        }
        return { ok: true };
      }

      case 'canvasVideo:drawFrame': {
        const jobId = data?.job_id;
        if (!jobId || !this._canvasVideoJobs?.has(jobId)) {
          return { ok: false, error: `job ${jobId} not registered` };
        }
        const width = Math.max(1, Math.min(4096, data?.width || 1920));
        const height = Math.max(1, Math.min(4096, data?.height || 1080));
        try {
          // Composition lives in the first child iframe of the render page.
          const children = this.browsingContext.children || [];
          if (!children[0]) {
            return { ok: false, error: 'no composition iframe in render page' };
          }
          const childBc = children[0];

          // Child WGP is populated asynchronously when the iframe loads
          // in a separate process (spike data: typically <50 ms, up to 3 s).
          let childWgp = childBc.currentWindowGlobal;
          const pollStart = Date.now();
          for (let i = 0; i < 60 && !childWgp; i++) {
            await new Promise((r) => setTimeout(r, 50));
            childWgp = childBc.currentWindowGlobal;
          }
          const pollMs = Date.now() - pollStart;
          if (!childWgp) {
            return { ok: false, error: `composition WGP null after ${pollMs} ms` };
          }

          const t0 = Date.now();
          const bitmap = await childWgp.drawSnapshot(
            new DOMRect(0, 0, width, height),
            1,
            'transparent'
          );
          const drawMs = Date.now() - t0;

          // hiddenDOMWindow is unavailable on Linux — use the most-recent
          // browser window's document to allocate an offscreen canvas.
          const browserWin = Services.wm.getMostRecentWindow('navigator:browser');
          if (!browserWin) {
            if (typeof bitmap.close === 'function') bitmap.close();
            return { ok: false, error: 'no navigator:browser window' };
          }
          const doc = browserWin.document;
          const canvas = doc.createElementNS(
            'http://www.w3.org/1999/xhtml',
            'html:canvas'
          );
          canvas.width = width;
          canvas.height = height;
          const ctx = canvas.getContext('2d');
          ctx.drawImage(bitmap, 0, 0);
          if (typeof bitmap.close === 'function') {
            bitmap.close();
          }

          const blob = await new Promise((resolve) =>
            canvas.toBlob(resolve, 'image/png')
          );
          if (!blob) {
            return { ok: false, error: 'canvas.toBlob returned null' };
          }
          const buf = await blob.arrayBuffer();

          return {
            ok: true,
            pollMs,
            drawMs,
            size: buf.byteLength,
            bytes: new Uint8Array(buf),
            width,
            height,
          };
        } catch (e) {
          return { ok: false, error: String(e && e.message ? e.message : e) };
        }
      }

      case 'contentStore:get': {
        const { key } = data;
        const val = lazy.NevofluxContentStore.get(key);
        console.log(
          `[NevofluxParent] contentStore:get key=${key} found=${!!val} type=${val?.type} contentLen=${val?.content?.length} state=${val?.state}`
        );
        return { value: val };
      }

      case 'contentStore:set': {
        const { key, value } = data;
        lazy.NevofluxContentStore.set(key, value);
        return { success: true };
      }

      case 'contentStore:delete': {
        const { key } = data;
        return { success: lazy.NevofluxContentStore.delete(key) };
      }

      case 'contentStore:query': {
        const { prefix } = data;
        return { results: lazy.NevofluxContentStore.query(prefix) };
      }

      case 'contentStore:subscribe': {
        const { key } = data;
        console.log(`[NevofluxParent] contentStore:subscribe key=${key}`);
        if (!this._contentStoreSubscriptions) {
          this._contentStoreSubscriptions = [];
        }
        const unsubscribe = lazy.NevofluxContentStore.subscribe(key, (value) => {
          try {
            console.log(
              `[NevofluxParent] contentStore:update pushing key=${key} contentLen=${value?.content?.length} state=${value?.state}`
            );
            this.sendAsyncMessage('contentStore:update', {
              type: 'contentStore:update',
              key,
              value,
            });
          } catch (e) {
            console.error(`[NevofluxParent] contentStore:update FAILED: ${e}`);
            // Actor destroyed, cleanup
            unsubscribe();
          }
        });
        this._contentStoreSubscriptions.push(unsubscribe);
        return { success: true };
      }

      case 'sidebar:open': {
        try {
          const result = await lazy.NevofluxBridgeRouter.request('sidebar:open', {});
          return { success: true, data: result };
        } catch (e) {
          return { success: false, error: { code: 13001, message: e.message } };
        }
      }

      case 'sidebar:sendMessage': {
        const { message } = data;
        try {
          const result = await lazy.NevofluxBridgeRouter.request('sidebar:sendMessage', {
            message,
          });
          return { success: true, data: result };
        } catch (e) {
          return { success: false, error: { code: 13001, message: e.message } };
        }
      }

      case 'sidebar:restoreSession': {
        const { sessionId } = data;
        try {
          const result = await lazy.NevofluxBridgeRouter.request('sidebar:restoreSession', {
            sessionId,
          });
          return { success: true, data: result };
        } catch (e) {
          return { success: false, error: { code: 13001, message: e.message } };
        }
      }

      case 'bridge:request': {
        const { type, payload } = data;
        const me = this;
        const onPush = (msg) => {
          try {
            me.sendAsyncMessage('bridge:push', { msg });
          } catch (_e) {
            // Actor destroyed — silently ignore (router will clean up).
          }
        };
        try {
          const result = await lazy.NevofluxBridgeRouter.request(type, payload, onPush);
          return { success: true, data: result };
        } catch (e) {
          return { success: false, error: { code: 13001, message: e.message } };
        }
      }

      case 'agent:subscribe': {
        const { sessionId } = data;
        if (!this._agentSessionUnsubs) {
          this._agentSessionUnsubs = new Map();
        }
        const unsub = lazy.NevofluxBridgeRouter.subscribe(sessionId, (message) => {
          try {
            this.sendAsyncMessage('agent:push', { sessionId, message });
          } catch (e) {
            // Actor destroyed
            unsub();
          }
        });
        this._agentSessionUnsubs.set(sessionId, unsub);
        return { success: true };
      }

      case 'agent:unsubscribe': {
        const { sessionId } = data;
        const unsub = this._agentSessionUnsubs?.get(sessionId);
        if (unsub) {
          unsub();
          this._agentSessionUnsubs.delete(sessionId);
        }
        return { success: true };
      }

      // EventBus persistent push channel. Bridge:request's 5-second push grace
      // is too short for long-lived event subscriptions, so subscribers open a
      // dedicated channel here (keyed by a client-chosen channelId) and close
      // it on unsubscribe. bridgePush in background.js targets this channelId.
      case 'events:channel_open': {
        const { channelId } = data;
        if (!this._eventChannelUnsubs) {
          this._eventChannelUnsubs = new Map();
        }
        if (this._eventChannelUnsubs.has(channelId)) {
          return { success: true, already_open: true };
        }
        const unsub = lazy.NevofluxBridgeRouter.subscribe(channelId, (msg) => {
          try {
            this.sendAsyncMessage('bridge:push', { msg });
          } catch (e) {
            unsub();
          }
        });
        this._eventChannelUnsubs.set(channelId, unsub);
        return { success: true };
      }

      case 'events:channel_close': {
        const { channelId } = data;
        const unsub = this._eventChannelUnsubs?.get(channelId);
        if (unsub) {
          unsub();
          this._eventChannelUnsubs.delete(channelId);
        }
        return { success: true };
      }

      default:
        return null;
    }
  }

  acceptDialog(text) {
    try {
      const win = this.browsingContext.topChromeWindow;
      const dialog = NevofluxParent._pendingDialogs.get(win);

      if (!dialog) {
        // No dialog present - silently succeed
        return { success: true };
      }

      // Handle prompt input
      if (text !== undefined && dialog.ui?.loginTextbox) {
        dialog.ui.loginTextbox.value = text;
      }

      // Click accept button
      if (dialog.ui?.button0) {
        dialog.ui.button0.click();
      }

      NevofluxParent._pendingDialogs.delete(win);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 11001, message: String(e), recoverable: false } };
    }
  }

  dismissDialog() {
    try {
      const win = this.browsingContext.topChromeWindow;
      const dialog = NevofluxParent._pendingDialogs.get(win);

      if (!dialog) {
        // No dialog present - silently succeed
        return { success: true };
      }

      // Click cancel button (button1) if exists, otherwise accept
      if (dialog.ui?.button1) {
        dialog.ui.button1.click();
      } else if (dialog.ui?.button0) {
        dialog.ui.button0.click();
      }

      NevofluxParent._pendingDialogs.delete(win);
      return { success: true };
    } catch (e) {
      return { success: false, error: { code: 11002, message: String(e), recoverable: false } };
    }
  }
}
