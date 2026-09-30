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

/** Whether a viewport point is inside a width×height viewport. */
export function pointInViewport(point, width, height) {
  return point.x >= 0 && point.y >= 0 && point.x < width && point.y < height;
}

/**
 * Pick where to click from per-point hit results ('target', 'ancestor',
 * 'offscreen' or { occluder }). A point on the target itself wins; a point
 * on an ancestor (a label around a hidden checkbox) only when none hits
 * the target. Never a covered point: a click there lands on the cover.
 */
export function classifyClickPoints(hits) {
  let index = hits.indexOf('target');
  if (index === -1) {
    index = hits.indexOf('ancestor');
  }
  if (index !== -1) {
    return { kind: 'target', index };
  }
  const cover = hits.find((h) => h && typeof h === 'object' && h.occluder);
  if (cover) {
    return { kind: 'covered', occluder: cover.occluder };
  }
  return { kind: 'offscreen' };
}

/**
 * A lower click tier runs only when the one above sent nothing. Once a press
 * reached the page, a second tier is a second click (a toggle flips back, a
 * payment is sent twice), even if no effect was seen.
 */
export function shouldTryNextTier(outcome) {
  return outcome !== 'pressed';
}

/** A short, model-readable name for the element covering a target. */
export function describeOccluder({ tagName, id, role, label }) {
  let s = String(tagName || 'element').toLowerCase();
  if (id) {
    s += `#${id}`;
  }
  if (role) {
    s += `[role=${role}]`;
  }
  const text = String(label || '')
    .replace(/\s+/g, ' ')
    .trim();
  if (text) {
    s += ` "${text.length > 40 ? text.slice(0, 37) + '...' : text}"`;
  }
  return s;
}

/**
 * The refusal the model reads. The daemon forwards only `error.message`, so
 * the advice has to be in it.
 */
export function coveredMessage(occluder) {
  return (
    `Element is covered by ${occluder}; the click was not sent. ` +
    'Dismiss or close that element first (for example accept the cookie banner ' +
    'or close the dialog), then take a new snapshot.'
  );
}

/** What a finished click reports about its effect. */
export function clickEffect({ method, changed, elementRemoved }) {
  if (method === 'all_tiers_exhausted') {
    return { effective: false, effect: 'not_dispatched' };
  }
  const effective = Boolean(changed || elementRemoved);
  return { effective, effect: effective ? 'observed' : 'none_observed' };
}

/**
 * When no point hit the target, scroll it into view once and pick again: a
 * target clipped by an inner scroll container is inside the window but its
 * points land outside the scroller, and it is not really covered.
 */
export function shouldRescrollAndRepick(pick, alreadyRescrolled) {
  return pick.kind !== 'target' && !alreadyRescrolled;
}

/**
 * Gecko accessible role names (nsIAccessibilityService.getStringRole, see
 * engine/accessible/base/RoleMap.inc) → the ARIA role the snapshot prints.
 * The actor used to map role *numbers* through a hand-written table that
 * matched no Gecko version, so buttons, fields and selects vanished.
 */
const GECKO_TO_ARIA = {
  pushbutton: 'button',
  'toggle button': 'button',
  buttonmenu: 'button',
  checkbutton: 'checkbox',
  radiobutton: 'radio',
  entry: 'textbox',
  'password text': 'textbox',
  combobox: 'combobox',
  editcombobox: 'combobox',
  'combobox list': 'listbox',
  listbox: 'listbox',
  'combobox option': 'option',
  'listbox option': 'option',
  'listbox rich option': 'option',
  link: 'link',
  switch: 'switch',
  slider: 'slider',
  spinbutton: 'spinbutton',
  pagetab: 'tab',
  menuitem: 'menuitem',
  'check menu item': 'menuitem',
  'radio menu item': 'menuitem',
  outlineitem: 'treeitem',
};

const INTERACTIVE = new Set(Object.values(GECKO_TO_ARIA));

/** The ARIA role for a Gecko role name, or '' for non-controls. */
export function canonicalRole(geckoName) {
  return GECKO_TO_ARIA[geckoName] || '';
}

/** Whether a canonical role is a control the snapshot lists. */
export function isInteractiveRole(role) {
  return INTERACTIVE.has(role);
}
