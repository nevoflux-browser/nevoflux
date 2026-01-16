/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Motion Utility
 * Animation helpers using Web Animations API
 */

/**
 * Easing functions
 */
export const easings = {
    linear: 'linear',
    easeIn: 'ease-in',
    easeOut: 'ease-out',
    easeInOut: 'ease-in-out',
    // Custom cubic-bezier easings
    spring: 'cubic-bezier(0.175, 0.885, 0.32, 1.275)',
    bounce: 'cubic-bezier(0.68, -0.55, 0.265, 1.55)',
    smooth: 'cubic-bezier(0.4, 0, 0.2, 1)',
};

/**
 * Animate an element
 * @param {HTMLElement} element - Element to animate
 * @param {Object} keyframes - Animation keyframes
 * @param {Object} options - Animation options
 * @returns {Animation} Web Animation object
 */
export function animate(element, keyframes, options = {}) {
    const defaultOptions = {
        duration: 300,
        easing: easings.smooth,
        fill: 'forwards',
    };

    return element.animate(keyframes, { ...defaultOptions, ...options });
}

/**
 * Fade in animation
 * @param {HTMLElement} element - Element to animate
 * @param {Object} options - Animation options
 * @returns {Animation}
 */
export function fadeIn(element, options = {}) {
    return animate(
        element,
        [
            { opacity: 0 },
            { opacity: 1 },
        ],
        { duration: 200, ...options }
    );
}

/**
 * Fade out animation
 * @param {HTMLElement} element - Element to animate
 * @param {Object} options - Animation options
 * @returns {Animation}
 */
export function fadeOut(element, options = {}) {
    return animate(
        element,
        [
            { opacity: 1 },
            { opacity: 0 },
        ],
        { duration: 200, ...options }
    );
}

/**
 * Slide in from bottom
 * @param {HTMLElement} element - Element to animate
 * @param {Object} options - Animation options
 * @returns {Animation}
 */
export function slideInUp(element, options = {}) {
    return animate(
        element,
        [
            { opacity: 0, transform: 'translateY(20px)' },
            { opacity: 1, transform: 'translateY(0)' },
        ],
        { duration: 300, easing: easings.spring, ...options }
    );
}

/**
 * Slide out to bottom
 * @param {HTMLElement} element - Element to animate
 * @param {Object} options - Animation options
 * @returns {Animation}
 */
export function slideOutDown(element, options = {}) {
    return animate(
        element,
        [
            { opacity: 1, transform: 'translateY(0)' },
            { opacity: 0, transform: 'translateY(20px)' },
        ],
        { duration: 200, ...options }
    );
}

/**
 * Scale in animation
 * @param {HTMLElement} element - Element to animate
 * @param {Object} options - Animation options
 * @returns {Animation}
 */
export function scaleIn(element, options = {}) {
    return animate(
        element,
        [
            { opacity: 0, transform: 'scale(0.95)' },
            { opacity: 1, transform: 'scale(1)' },
        ],
        { duration: 200, easing: easings.spring, ...options }
    );
}

/**
 * Scale out animation
 * @param {HTMLElement} element - Element to animate
 * @param {Object} options - Animation options
 * @returns {Animation}
 */
export function scaleOut(element, options = {}) {
    return animate(
        element,
        [
            { opacity: 1, transform: 'scale(1)' },
            { opacity: 0, transform: 'scale(0.95)' },
        ],
        { duration: 150, ...options }
    );
}

/**
 * Shake animation (for errors)
 * @param {HTMLElement} element - Element to animate
 * @param {Object} options - Animation options
 * @returns {Animation}
 */
export function shake(element, options = {}) {
    return animate(
        element,
        [
            { transform: 'translateX(0)' },
            { transform: 'translateX(-8px)' },
            { transform: 'translateX(8px)' },
            { transform: 'translateX(-8px)' },
            { transform: 'translateX(8px)' },
            { transform: 'translateX(0)' },
        ],
        { duration: 400, easing: easings.easeOut, ...options }
    );
}

/**
 * Pulse animation
 * @param {HTMLElement} element - Element to animate
 * @param {Object} options - Animation options
 * @returns {Animation}
 */
export function pulse(element, options = {}) {
    return animate(
        element,
        [
            { transform: 'scale(1)' },
            { transform: 'scale(1.05)' },
            { transform: 'scale(1)' },
        ],
        { duration: 300, ...options }
    );
}

/**
 * Stagger animations for lists
 * @param {NodeList|Array} elements - Elements to animate
 * @param {Function} animationFn - Animation function to apply
 * @param {number} staggerDelay - Delay between each element (ms)
 * @returns {Promise<void>}
 */
export async function stagger(elements, animationFn, staggerDelay = 50) {
    const animations = [];

    for (let i = 0; i < elements.length; i++) {
        await new Promise(resolve => setTimeout(resolve, staggerDelay));
        animations.push(animationFn(elements[i]));
    }

    return Promise.all(animations.map(a => a.finished));
}

/**
 * Wait for animation to complete
 * @param {Animation} animation - Animation to wait for
 * @returns {Promise<void>}
 */
export function waitForAnimation(animation) {
    return animation.finished;
}
