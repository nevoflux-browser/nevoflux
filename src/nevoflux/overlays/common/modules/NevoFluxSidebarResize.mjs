// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * JavaScript-based sidebar resize handler for NevoFlux.
 *
 * The native XUL <splitter> resize mechanism (nsSplitterFrame) does not
 * reliably work when the splitter lives inside a CSS flex container
 * (#tabbrowser-tabbox has display:flex set by Zen). On Windows this
 * causes the drag gesture to be recognized (cursor changes) but the
 * sidebar width never updates.
 *
 * This module intercepts pointer events on #sidebar-splitter and
 * directly sets the width of #sidebar-box, bypassing nsSplitterFrame.
 */

const MIN_WIDTH = 300;
const MAX_WIDTH = 960;

class NevoFluxSidebarResize {
  #splitter = null;
  #box = null;
  #dragging = false;
  #startX = 0;
  #startWidth = 0;

  constructor() {
    document.addEventListener('MozBeforeInitialXULLayout', () => this.#init(), { once: true });
  }

  #init() {
    // Defer until the DOM is fully ready (splitter may be moved by ZenStartup)
    window.addEventListener(
      'load',
      () => {
        this.#splitter = document.getElementById('sidebar-splitter');
        this.#box = document.getElementById('sidebar-box');
        if (!this.#splitter || !this.#box) return;
        this.#splitter.addEventListener('mousedown', this);
      },
      { once: true }
    );
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

    // Listen on the window so we catch events even when the pointer
    // moves outside the splitter.
    window.addEventListener('mousemove', this);
    window.addEventListener('mouseup', this);
  }

  #onMouseMove(event) {
    if (!this.#dragging) return;
    event.preventDefault();
    const delta = event.screenX - this.#startX;

    // Sidebar is on the right: dragging left (negative delta) = wider
    const newWidth = Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, this.#startWidth - delta)));
    this.#box.style.width = newWidth + 'px';
  }

  #onMouseUp(event) {
    if (!this.#dragging) return;
    this.#dragging = false;
    window.removeEventListener('mousemove', this);
    window.removeEventListener('mouseup', this);

    // Persist the width to the pref so it survives restarts
    const width = parseInt(this.#box.style.width, 10);
    if (width) {
      Services.prefs.setIntPref('sidebar.width', width);
    }
  }
}

new NevoFluxSidebarResize();
