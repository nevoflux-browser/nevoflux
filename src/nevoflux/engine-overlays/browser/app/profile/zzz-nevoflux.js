/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * NevoFlux Browser Preferences
 */

// Sidebar positioning
// sidebar.position_start=false puts #sidebar-box on the right (for NevoFlux agent).
// CSS overrides in zen-sidebar-right.css prevent #sidebar-main from also moving right.
pref('sidebar.position_start', false);
pref('sidebar.width', 500);
pref('sidebar.minWidth', 300);
pref('sidebar.maxWidth', 960);

// Zen vertical tabs positioning (true = right side)
// Keep default (false) so the Zen sidebar stays on the left.
pref('zen.tabs.vertical.right-side', false);

// NevoFlux Agent settings
pref('extensions.nevoflux.sidebar.position', 'right');
pref('extensions.nevoflux.sidebar.width', 500);
pref('extensions.nevoflux.sidebar.resizable', true);
