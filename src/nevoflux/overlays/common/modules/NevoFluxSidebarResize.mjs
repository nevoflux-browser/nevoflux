// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * JavaScript-based sidebar resize handler for NevoFlux.
 *
 * The native XUL <splitter> resize mechanism (nsSplitterFrame) does not
 * reliably work when the splitter lives inside a CSS flex container
 * (#tabbrowser-tabbox has display:flex set by Zen). On Linux/Windows the
 * native handler also resets the width on mouseup by setting both the
 * XUL `width` attribute and inline `style.width` on #sidebar-box.
 *
 * This module:
 *  1. Intercepts pointer events on #sidebar-splitter for drag-to-resize.
 *  2. Uses a MutationObserver to immediately undo any width changes made
 *     by nsSplitterFrame (both the `width` attribute and inline style).
 *  3. Persists width to `sidebar.width` pref and restores it on startup.
 */

const MIN_WIDTH = 300;
const MAX_WIDTH = 960;
const DEFAULT_WIDTH = 500;
const PREF_KEY = 'sidebar.width';

class NevoFluxSidebarResize {
  #splitter = null;
  #box = null;
  #dragging = false;
  #startX = 0;
  #startWidth = 0;
  /** The width we want — MutationObserver restores this when native code interferes. */
  #desiredWidth = DEFAULT_WIDTH;
  /** Guard flag to ignore MutationObserver callbacks triggered by our own changes. */
  #selfUpdate = false;

  constructor() {
    document.addEventListener('MozBeforeInitialXULLayout', () => this.#init(), { once: true });
  }

  #init() {
    window.addEventListener(
      'load',
      () => {
        this.#splitter = document.getElementById('sidebar-splitter');
        this.#box = document.getElementById('sidebar-box');
        if (!this.#splitter || !this.#box) return;

        // Restore persisted width on startup
        this.#restoreWidth();

        // Guard against nsSplitterFrame (C++) resetting the width.
        // It sets both the XUL `width` attribute AND replaces inline style
        // on #sidebar-box during mouseup. We watch for both and undo them.
        new MutationObserver(() => {
          if (this.#selfUpdate) return;
          // Remove XUL width attribute if set
          if (this.#box.hasAttribute('width')) {
            this.#selfUpdate = true;
            this.#box.removeAttribute('width');
            this.#selfUpdate = false;
          }
          // Re-apply our desired width if it was overwritten
          const current = this.#box.style.getPropertyValue('width');
          const desired = this.#desiredWidth + 'px';
          if (current !== desired || this.#box.style.getPropertyPriority('width') !== 'important') {
            this.#selfUpdate = true;
            this.#box.style.setProperty('width', desired, 'important');
            this.#selfUpdate = false;
          }
        }).observe(this.#box, { attributes: true, attributeFilter: ['width', 'style'] });

        this.#splitter.addEventListener('mousedown', this);
      },
      { once: true }
    );
  }

  #restoreWidth() {
    let width = DEFAULT_WIDTH;
    try {
      if (Services.prefs.prefHasUserValue(PREF_KEY)) {
        width = Services.prefs.getIntPref(PREF_KEY, DEFAULT_WIDTH);
      }
    } catch (e) {
      // pref not set — use default
    }
    this.#desiredWidth = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width));
    this.#applyWidth();
  }

  #applyWidth() {
    this.#selfUpdate = true;
    this.#box.removeAttribute('width');
    this.#box.style.setProperty('width', this.#desiredWidth + 'px', 'important');
    this.#selfUpdate = false;
  }

  handleEvent(event) {
    switch (event.type) {
      case 'mousedown':
        this.#onMouseDown(event);
        break;
      case 'mousemove':
        this.#onMouseMove(event);
        break;
      case 'mouseup':
        this.#onMouseUp(event);
        break;
    }
  }

  #onMouseDown(event) {
    if (event.button !== 0) return;
    event.preventDefault();
    this.#dragging = true;
    this.#startX = event.screenX;
    this.#startWidth = this.#box.getBoundingClientRect().width;
    window.addEventListener('mousemove', this);
    window.addEventListener('mouseup', this);
  }

  #onMouseMove(event) {
    if (!this.#dragging) return;
    event.preventDefault();
    const delta = event.screenX - this.#startX;
    // Sidebar is on the right: dragging left (negative delta) = wider
    this.#desiredWidth = Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, this.#startWidth - delta)));
    this.#applyWidth();
  }

  #onMouseUp(_event) {
    if (!this.#dragging) return;
    this.#dragging = false;
    window.removeEventListener('mousemove', this);
    window.removeEventListener('mouseup', this);

    // Persist width
    if (this.#desiredWidth >= MIN_WIDTH && this.#desiredWidth <= MAX_WIDTH) {
      Services.prefs.setIntPref(PREF_KEY, this.#desiredWidth);
    }
  }
}

new NevoFluxSidebarResize();
