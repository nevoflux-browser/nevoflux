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
