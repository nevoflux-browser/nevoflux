/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Decisions behind NevofluxChild's page actions, kept free of DOM and
 * ChromeUtils so node unit tests import this exact file.
 */

const SECRET_INPUT_TYPES = new Set(['password', 'file', 'hidden']);

/**
 * The value a snapshot line may show for a node, or null for none. A
 * snapshot goes to the model provider, so password, file and hidden inputs
 * never show theirs.
 */
export function snapshotValue({ tagName, type, value }) {
  if (typeof value !== 'string' || value === '') {
    return null;
  }
  if (
    String(tagName).toUpperCase() === 'INPUT' &&
    SECRET_INPUT_TYPES.has(String(type || '').toLowerCase())
  ) {
    return null;
  }
  return value.length > 30 ? value.slice(0, 27) + '...' : value;
}

/**
 * Attributes whose change after a click counts as its effect. Includes the
 * state attributes a toggle, a pressed button or a disclosure flips; without
 * them a checkbox tick looked ineffective and got clicked again (unticked).
 */
export const EFFECT_ATTRIBUTES = [
  'class',
  'style',
  'hidden',
  'disabled',
  'open',
  'aria-hidden',
  'aria-expanded',
  'aria-selected',
  'aria-checked',
  'aria-pressed',
  'aria-current',
  'data-state',
  'data-active',
];

const HOVER_CLASSES = /\b(hover|active|focus|focused|pressed|highlighted)\b/gi;

function cleanClasses(value) {
  return String(value || '')
    .replace(HOVER_CLASSES, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Whether an observed attribute mutation is an effect of the click. */
export function isMeaningfulAttributeChange(attr, oldValue, newValue) {
  if (attr === 'style') {
    return false;
  }
  if (attr === 'class') {
    return cleanClasses(oldValue) !== cleanClasses(newValue);
  }
  return oldValue !== newValue;
}

const NON_TEXT_INPUT_TYPES = new Set([
  'button',
  'checkbox',
  'color',
  'file',
  'hidden',
  'image',
  'radio',
  'range',
  'reset',
  'submit',
]);

/** Whether focusing this element is itself the point of clicking it. */
export function isTextEditable({ tagName, type, isContentEditable }) {
  const tag = String(tagName || '').toUpperCase();
  if (tag === 'TEXTAREA') {
    return true;
  }
  if (tag === 'INPUT') {
    return !NON_TEXT_INPUT_TYPES.has(String(type || 'text').toLowerCase());
  }
  return isContentEditable === true;
}

const CONTROL_STATE_KEYS = [
  'checked',
  'value',
  'selectedIndex',
  'open',
  'ariaChecked',
  'ariaPressed',
  'ariaExpanded',
  'focused',
];

/** Whether a control's state moved between two `_controlState` readings. */
export function controlStateChanged(before, after) {
  if (!before || !after) {
    return false;
  }
  return CONTROL_STATE_KEYS.some((k) => before[k] !== after[k]);
}
