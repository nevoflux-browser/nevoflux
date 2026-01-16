/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Keyboard Utility
 * Keyboard shortcut management
 */

/**
 * Key codes mapping
 */
export const Keys = {
    ENTER: 'Enter',
    ESCAPE: 'Escape',
    TAB: 'Tab',
    SPACE: ' ',
    ARROW_UP: 'ArrowUp',
    ARROW_DOWN: 'ArrowDown',
    ARROW_LEFT: 'ArrowLeft',
    ARROW_RIGHT: 'ArrowRight',
    BACKSPACE: 'Backspace',
    DELETE: 'Delete',
    HOME: 'Home',
    END: 'End',
    PAGE_UP: 'PageUp',
    PAGE_DOWN: 'PageDown',
};

/**
 * Parse a shortcut string into components
 * @param {string} shortcut - e.g., "Ctrl+Shift+Enter"
 * @returns {Object} Parsed shortcut
 */
export function parseShortcut(shortcut) {
    const parts = shortcut.toLowerCase().split('+');
    return {
        ctrl: parts.includes('ctrl') || parts.includes('control'),
        alt: parts.includes('alt'),
        shift: parts.includes('shift'),
        meta: parts.includes('meta') || parts.includes('cmd') || parts.includes('command'),
        key: parts[parts.length - 1].toUpperCase(),
    };
}

/**
 * Check if a keyboard event matches a shortcut
 * @param {KeyboardEvent} event - Keyboard event
 * @param {string|Object} shortcut - Shortcut string or parsed shortcut
 * @returns {boolean}
 */
export function matchesShortcut(event, shortcut) {
    const parsed = typeof shortcut === 'string' ? parseShortcut(shortcut) : shortcut;

    return (
        event.ctrlKey === parsed.ctrl &&
        event.altKey === parsed.alt &&
        event.shiftKey === parsed.shift &&
        event.metaKey === parsed.meta &&
        event.key.toUpperCase() === parsed.key
    );
}

/**
 * Keyboard shortcut manager
 */
class KeyboardManager {
    constructor() {
        this.shortcuts = new Map();
        this.enabled = true;
        this._handleKeyDown = this._handleKeyDown.bind(this);
    }

    /**
     * Initialize keyboard listener
     * @param {HTMLElement|Document} target - Target element
     */
    init(target = document) {
        target.addEventListener('keydown', this._handleKeyDown);
    }

    /**
     * Clean up keyboard listener
     * @param {HTMLElement|Document} target - Target element
     */
    destroy(target = document) {
        target.removeEventListener('keydown', this._handleKeyDown);
    }

    /**
     * Register a shortcut
     * @param {string} shortcut - Shortcut string (e.g., "Ctrl+Enter")
     * @param {Function} handler - Handler function
     * @param {Object} options - Options
     */
    register(shortcut, handler, options = {}) {
        const parsed = parseShortcut(shortcut);
        const key = this._getShortcutKey(parsed);

        if (!this.shortcuts.has(key)) {
            this.shortcuts.set(key, []);
        }

        this.shortcuts.get(key).push({
            parsed,
            handler,
            preventDefault: options.preventDefault ?? true,
            stopPropagation: options.stopPropagation ?? false,
        });
    }

    /**
     * Unregister a shortcut
     * @param {string} shortcut - Shortcut string
     * @param {Function} handler - Handler to remove (optional)
     */
    unregister(shortcut, handler) {
        const parsed = parseShortcut(shortcut);
        const key = this._getShortcutKey(parsed);

        if (handler) {
            const handlers = this.shortcuts.get(key);
            if (handlers) {
                const index = handlers.findIndex(h => h.handler === handler);
                if (index > -1) {
                    handlers.splice(index, 1);
                }
            }
        } else {
            this.shortcuts.delete(key);
        }
    }

    /**
     * Enable or disable keyboard shortcuts
     * @param {boolean} enabled
     */
    setEnabled(enabled) {
        this.enabled = enabled;
    }

    _handleKeyDown(event) {
        if (!this.enabled) return;

        // Skip if user is typing in an input
        const target = event.target;
        const isInput = target.tagName === 'INPUT' ||
            target.tagName === 'TEXTAREA' ||
            target.isContentEditable;

        for (const [, handlers] of this.shortcuts) {
            for (const { parsed, handler, preventDefault, stopPropagation } of handlers) {
                if (matchesShortcut(event, parsed)) {
                    // Allow shortcuts in inputs if they include modifiers
                    if (isInput && !parsed.ctrl && !parsed.alt && !parsed.meta) {
                        continue;
                    }

                    if (preventDefault) {
                        event.preventDefault();
                    }
                    if (stopPropagation) {
                        event.stopPropagation();
                    }

                    try {
                        handler(event);
                    } catch (error) {
                        console.error('Error in keyboard shortcut handler:', error);
                    }
                    return;
                }
            }
        }
    }

    _getShortcutKey(parsed) {
        const parts = [];
        if (parsed.ctrl) parts.push('ctrl');
        if (parsed.alt) parts.push('alt');
        if (parsed.shift) parts.push('shift');
        if (parsed.meta) parts.push('meta');
        parts.push(parsed.key);
        return parts.join('+');
    }
}

// Export singleton instance
export const keyboardManager = new KeyboardManager();

/**
 * Decorator for keyboard shortcuts in Lit components
 * @param {string} shortcut - Shortcut string
 * @param {Object} options - Options
 */
export function onShortcut(shortcut, options = {}) {
    return function (target, propertyKey, descriptor) {
        const originalConnectedCallback = target.connectedCallback;
        const originalDisconnectedCallback = target.disconnectedCallback;

        target.connectedCallback = function () {
            if (originalConnectedCallback) {
                originalConnectedCallback.call(this);
            }
            keyboardManager.register(shortcut, descriptor.value.bind(this), options);
        };

        target.disconnectedCallback = function () {
            keyboardManager.unregister(shortcut, descriptor.value.bind(this));
            if (originalDisconnectedCallback) {
                originalDisconnectedCallback.call(this);
            }
        };

        return descriptor;
    };
}
