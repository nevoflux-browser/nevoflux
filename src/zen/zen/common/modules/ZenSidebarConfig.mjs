/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * NevoFlux Sidebar Configuration
 * Positions sidebar on right with width settings for NevoFlux Agent
 */

const SIDEBAR_CONFIG = {
  minWidth: '300px',
  width: '50vw',
  maxWidth: '90vw',
  widthPx: typeof window !== 'undefined' ? Math.floor(window.innerWidth * 0.5) : 960,
};

export class ZenSidebarConfig {
  constructor() {
    if (document.readyState === 'complete') {
      this.#configure();
    } else {
      window.addEventListener('load', () => this.#configure(), { once: true });
    }
  }

  #configure() {
    this.#positionSidebarRight();
    this.#setSidebarDimensions();
    this.#persistPreferences();
  }

  #positionSidebarRight() {
    const tabbox = document.getElementById('tabbrowser-tabbox');
    tabbox?.setAttribute('sidebar-positionend', 'true');
  }

  #setSidebarDimensions() {
    const sidebarBox = document.getElementById('sidebar-box');
    if (!sidebarBox) return;

    Object.assign(sidebarBox.style, {
      minWidth: SIDEBAR_CONFIG.minWidth,
      width: SIDEBAR_CONFIG.width,
      maxWidth: SIDEBAR_CONFIG.maxWidth,
    });
  }

  #persistPreferences() {
    if (typeof Services === 'undefined' || !Services.prefs) return;

    Services.prefs.setBoolPref('sidebar.position_start', false);
    Services.prefs.setIntPref('sidebar.width', SIDEBAR_CONFIG.widthPx);
  }
}

// Auto-initialize
if (typeof window !== 'undefined') {
  new ZenSidebarConfig();
}
