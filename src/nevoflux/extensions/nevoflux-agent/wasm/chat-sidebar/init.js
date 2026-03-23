
// Maximize button click handler - MUST be synchronous to preserve user gesture
(function() {
    document.addEventListener('click', function(event) {
        const button = event.target.closest('.maximize-btn');
        if (!button) return;

        const urlParams = new URLSearchParams(window.location.search);
        if (urlParams.get('mode') === 'maximized') return;

        event.preventDefault();
        event.stopPropagation();

        console.log('[NevoFlux] Maximize clicked - synchronous handler');

        // Get data SYNCHRONOUSLY from window globals (set by WASM)
        const sessionId = window.__nevoflux_session_id || '';
        const targetTabId = window.__nevoflux_target_tab_id || 0;

        // Build URL synchronously
        const baseUrl = window.location.href.split('?')[0];
        const newUrl = `${baseUrl}?mode=maximized&session_id=${sessionId}&target_tab_id=${targetTabId}&source_tab_id=${targetTabId}`;

        console.log('[NevoFlux] Opening maximized view:', newUrl);

        // Open tab FIRST using window.open (synchronous, executes before close destroys context)
        window.open(newUrl, '_blank');

        // Then close sidebar (user gesture still valid since no await yet)
        if (browser.sidebarAction && browser.sidebarAction.close) {
            browser.sidebarAction.close().catch(e => console.warn('[NevoFlux] close failed:', e));
        }
    }, true); // capture phase
})();

// Restore button click handler - MUST be synchronous to preserve user gesture
(function() {
    document.addEventListener('click', function(event) {
        const button = event.target.closest('.restore-btn');
        if (!button) return;

        const urlParams = new URLSearchParams(window.location.search);
        if (urlParams.get('mode') !== 'maximized') return;

        event.preventDefault();
        event.stopPropagation();

        console.log('[NevoFlux] Restore clicked - synchronous handler');

        // Get source_tab_id from URL params
        const sourceTabId = parseInt(urlParams.get('source_tab_id')) || 0;

        // Open sidebar FIRST - this requires user gesture
        if (browser.sidebarAction && browser.sidebarAction.open) {
            browser.sidebarAction.open().catch(e => console.warn('[NevoFlux] open failed:', e));
        }

        // Then activate source tab and close this tab (no user gesture needed)
        if (sourceTabId > 0) {
            browser.tabs.update(sourceTabId, { active: true }).catch(e => console.warn('[NevoFlux] activate tab failed:', e));
        }

        // Close current maximized tab
        browser.tabs.getCurrent().then(tab => {
            if (tab && tab.id) {
                browser.tabs.remove(tab.id).catch(e => console.warn('[NevoFlux] close tab failed:', e));
            }
        }).catch(e => console.warn('[NevoFlux] get current tab failed:', e));
    }, true); // capture phase
})();

import init, * as bindings from './chat-sidebar-7c60dbab37ddda35.js';
const wasm = await init({ module_or_path: './chat-sidebar-7c60dbab37ddda35_bg.wasm' });


window.wasmBindings = bindings;


dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));