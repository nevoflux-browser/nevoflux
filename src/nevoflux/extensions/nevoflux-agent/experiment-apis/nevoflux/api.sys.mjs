/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * NevoFlux Experiment API Implementation
 *
 * This module provides a bridge between the Dioxus/WASM sidebar and
 * JSWindowActors for page interaction capabilities.
 */

const lazy = {};

ChromeUtils.defineESModuleGetters(lazy, {
  SessionStore: "resource:///modules/sessionstore/SessionStore.sys.mjs",
});

// Default timeout for tab restoration (ms)
const DEFAULT_RESTORE_TIMEOUT = 10000;

/**
 * Get native tab from extension tabManager
 * @param {number} tabId - The tab ID
 * @param {object} extension - The extension context
 * @returns {object|null} Native tab element or null
 */
function getNativeTab(tabId, extension) {
  const tabWrapper = extension.tabManager.get(tabId);
  if (!tabWrapper) {
    return null;
  }
  return tabWrapper.nativeTab;
}

/**
 * Get the linkedBrowser from a native tab
 * @param {object} nativeTab - The native tab element
 * @returns {object|null} Browser element or null
 */
function getBrowser(nativeTab) {
  if (!nativeTab) {
    return null;
  }
  return nativeTab.linkedBrowser || null;
}

/**
 * Get the Nevoflux actor from a native tab's browsingContext
 * @param {object} nativeTab - The native tab element
 * @returns {object|null} Nevoflux actor or null
 */
function getActor(nativeTab) {
  const browser = getBrowser(nativeTab);
  if (!browser) {
    return null;
  }

  const bc = browser.browsingContext;
  if (!bc) {
    return null;
  }

  const windowGlobal = bc.currentWindowGlobal;
  if (!windowGlobal) {
    return null;
  }

  try {
    return windowGlobal.getActor("Nevoflux");
  } catch (e) {
    // Actor not available
    return null;
  }
}

/**
 * Check if a tab is discarded (has "pending" attribute)
 * @param {object} nativeTab - The native tab element
 * @returns {boolean} True if tab is discarded
 */
function isTabDiscarded(nativeTab) {
  if (!nativeTab) {
    return false;
  }
  return nativeTab.hasAttribute("pending");
}

/**
 * Restore a discarded tab using SessionStore.restoreTabContent
 * This does NOT switch the visible tab - it restores the tab in the background.
 * @param {object} nativeTab - The native tab element
 * @param {number} timeout - Maximum time to wait for restoration (ms)
 * @returns {Promise<boolean>} True if restoration successful
 */
async function restoreTabIfNeeded(nativeTab, timeout = DEFAULT_RESTORE_TIMEOUT) {
  if (!nativeTab) {
    return false;
  }

  // Check if tab is actually discarded
  if (!isTabDiscarded(nativeTab)) {
    return true; // Already restored
  }

  return new Promise((resolve) => {
    let timeoutId;

    const onRestored = () => {
      if (timeoutId) {
        nativeTab.ownerGlobal.clearTimeout(timeoutId);
      }
      nativeTab.removeEventListener("SSTabRestored", onRestored);
      resolve(true);
    };

    // Set up timeout
    timeoutId = nativeTab.ownerGlobal.setTimeout(() => {
      nativeTab.removeEventListener("SSTabRestored", onRestored);
      resolve(false);
    }, timeout);

    // Listen for restoration complete
    nativeTab.addEventListener("SSTabRestored", onRestored);

    // Trigger restoration (does NOT switch tabs)
    try {
      lazy.SessionStore.restoreTabContent(nativeTab);
    } catch (e) {
      nativeTab.removeEventListener("SSTabRestored", onRestored);
      if (timeoutId) {
        nativeTab.ownerGlobal.clearTimeout(timeoutId);
      }
      resolve(false);
    }
  });
}

/**
 * Wait for a tab's browsingContext to become available
 * @param {object} nativeTab - The native tab element
 * @param {number} maxWaitMs - Maximum wait time in ms
 * @returns {Promise<object|null>} BrowsingContext or null
 */
async function waitForBrowsingContext(nativeTab, maxWaitMs = 2000) {
  const browser = getBrowser(nativeTab);
  if (!browser) {
    return null;
  }

  const waitInterval = 100;
  let waited = 0;

  while (waited < maxWaitMs) {
    if (browser.browsingContext?.currentWindowGlobal) {
      return browser.browsingContext;
    }

    await new Promise(resolve =>
      nativeTab.ownerGlobal.setTimeout(resolve, waitInterval)
    );
    waited += waitInterval;
  }

  return browser.browsingContext || null;
}

export class nevoflux extends ExtensionAPI {
  getAPI(context) {
    const { extension } = context;

    return {
      nevoflux: {
        /**
         * Get the current state of a tab
         * @param {number} tabId - The tab ID
         * @returns {Promise<object>} TabState object
         */
        async getTabState(tabId) {
          const nativeTab = getNativeTab(tabId, extension);
          if (!nativeTab) {
            throw new Error(`Tab not found: ${tabId}`);
          }

          const browser = getBrowser(nativeTab);
          const discarded = isTabDiscarded(nativeTab);

          // Determine loading status
          let status = "unloaded";
          if (!discarded && browser) {
            const webProgress = browser.webProgress;
            if (webProgress?.isLoadingDocument) {
              status = "loading";
            } else if (browser.browsingContext?.currentWindowGlobal) {
              status = "complete";
            }
          }

          return {
            discarded,
            status,
            url: browser?.currentURI?.spec || "",
            title: browser?.contentTitle || nativeTab.label || "",
          };
        },

        /**
         * Get page content from a tab as markdown, HTML, or text
         * @param {number} tabId - The tab ID
         * @param {object} options - Options for content extraction
         * @returns {Promise<object>} TabContent object
         */
        async getTabContent(tabId, options = {}) {
          const {
            format = "markdown",
            selector = null,
            autoRestore = true,
            keepRestored = true,
            timeout = DEFAULT_RESTORE_TIMEOUT,
          } = options;

          const nativeTab = getNativeTab(tabId, extension);
          if (!nativeTab) {
            throw new Error(`Tab not found: ${tabId}`);
          }

          const wasDiscarded = isTabDiscarded(nativeTab);

          // Auto-restore discarded tabs if needed
          if (wasDiscarded && autoRestore) {
            const restored = await restoreTabIfNeeded(nativeTab, timeout);
            if (!restored) {
              throw new Error("Failed to restore discarded tab");
            }

            // Wait for browsingContext to be ready
            const bc = await waitForBrowsingContext(nativeTab, 2000);
            if (!bc?.currentWindowGlobal) {
              throw new Error("Tab restored but browsingContext not ready");
            }
          }

          // Get the actor
          const actor = getActor(nativeTab);
          if (!actor) {
            throw new Error("Cannot get actor - tab may not be fully loaded");
          }

          const browser = getBrowser(nativeTab);
          let content = "";
          let title = "";
          let url = "";

          // Call the appropriate actor action based on format
          if (format === "markdown") {
            const result = await actor.sendQuery("execute", {
              action: "getMarkdown",
              params: { selector },
            });

            if (result.success === false) {
              throw new Error(result.error?.message || "Failed to get markdown");
            }

            content = result.markdown || "";
            title = result.title || "";
            url = result.url || "";
          } else if (format === "html") {
            const result = await actor.sendQuery("execute", {
              action: "getHtml",
              params: { selector: selector || "body" },
            });

            // getHtml returns the HTML string directly
            content = typeof result === "string" ? result : (result || "");
            title = browser?.contentTitle || "";
            url = browser?.currentURI?.spec || "";
          } else if (format === "text") {
            const result = await actor.sendQuery("execute", {
              action: "getText",
              params: { selector: selector || "body" },
            });

            // getText returns the text string directly
            content = typeof result === "string" ? result : (result || "");
            title = browser?.contentTitle || "";
            url = browser?.currentURI?.spec || "";
          } else {
            throw new Error(`Unsupported format: ${format}`);
          }

          return {
            tabId,
            url,
            title,
            content,
            format,
            extractedAt: Date.now(),
            wasDiscarded,
          };
        },

        /**
         * Start element picker and wait for user selection
         * @param {number} tabId - The tab ID
         * @param {object} options - Picker options
         * @returns {Promise<object>} PickerResult object
         */
        async pickElement(tabId, options = {}) {
          const {
            hint = "",
            filter = "any",
            timeout = 60000,
            highlightColor = "#6366f1",
          } = options;

          const nativeTab = getNativeTab(tabId, extension);
          if (!nativeTab) {
            throw new Error(`Tab not found: ${tabId}`);
          }

          // Restore tab if needed
          await restoreTabIfNeeded(nativeTab, 30000);

          const actor = getActor(nativeTab);
          if (!actor) {
            throw new Error("Cannot get actor - tab may not be fully loaded");
          }

          // Start picker with timeout
          const timeoutPromise = new Promise((_, reject) => {
            setTimeout(() => reject(new Error("Picker timeout")), timeout);
          });

          const pickerPromise = actor.sendQuery("startPicker", {
            filter,
            highlightColor,
          });

          try {
            const result = await Promise.race([pickerPromise, timeoutPromise]);
            if (!result.success) {
              throw new Error(result.error || "Picker failed");
            }
            return result.data;
          } catch (e) {
            // Ensure picker is stopped on error
            try {
              await actor.sendQuery("stopPicker", {});
            } catch {}
            throw e;
          }
        },

        /**
         * Cancel active element picker
         * @param {number} tabId - The tab ID
         * @returns {Promise<void>}
         */
        async cancelPicker(tabId) {
          const nativeTab = getNativeTab(tabId, extension);
          if (!nativeTab) {
            throw new Error(`Tab not found: ${tabId}`);
          }

          const actor = getActor(nativeTab);
          if (!actor) {
            throw new Error("Cannot get actor - tab may not be fully loaded");
          }

          await actor.sendQuery("stopPicker", {});
        },

        /**
         * Get current text selection from a tab
         * @param {number} tabId - The tab ID
         * @returns {Promise<object|null>} SelectionData or null if no selection
         */
        async getSelection(tabId) {
          const nativeTab = getNativeTab(tabId, extension);
          if (!nativeTab) {
            throw new Error(`Tab not found: ${tabId}`);
          }

          if (isTabDiscarded(nativeTab)) {
            return null;
          }

          const actor = getActor(nativeTab);
          if (!actor) {
            return null;
          }

          const result = await actor.sendQuery("getSelection", {});

          return result.success ? result.data : null;
        },

        /**
         * Lock page to prevent user interaction during agent operations
         * @param {number} tabId - The tab ID
         * @param {object} options - Lock options
         * @returns {Promise<void>}
         */
        async lockPage(tabId, options = {}) {
          const { showOverlay = true, message = "" } = options;

          const nativeTab = getNativeTab(tabId, extension);
          if (!nativeTab) {
            throw new Error(`Tab not found: ${tabId}`);
          }

          const actor = getActor(nativeTab);
          if (!actor) {
            throw new Error("Actor not available for this tab");
          }

          await actor.sendQuery("lockPage", { showOverlay, message });
        },

        /**
         * Unlock page after agent operations
         * @param {number} tabId - The tab ID
         * @returns {Promise<void>}
         */
        async unlockPage(tabId) {
          const nativeTab = getNativeTab(tabId, extension);
          if (!nativeTab) {
            throw new Error(`Tab not found: ${tabId}`);
          }

          const actor = getActor(nativeTab);
          if (!actor) {
            throw new Error("Actor not available for this tab");
          }

          await actor.sendQuery("unlockPage", {});
        },

        /**
         * Event: Fired when text selection changes in any tab
         */
        onSelectionChanged: new ExtensionCommon.EventManager({
          context,
          name: "nevoflux.onSelectionChanged",
          register: (fire) => {
            // Stub: event registration not yet implemented
            const listener = (tabId, selection) => {
              fire.async(tabId, selection);
            };

            // TODO: Register selection change observer

            return () => {
              // TODO: Unregister selection change observer
            };
          },
        }).api(),
      },
    };
  }
}
