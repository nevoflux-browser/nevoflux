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

/** 'e12', '12' or 12 → 'e12'. */
export function normalizeRefId(id) {
  const s = String(id).trim();
  return s.startsWith('e') ? s : `e${s}`;
}

/**
 * Element identity for one document: a node keeps its id for as long as the
 * document lives (ids are never reused), and an id resolves to that exact
 * node — never to whatever now matches a selector. Holds nodes weakly.
 */
export class RefRegistry {
  constructor() {
    this._next = 0;
    this._ids = new WeakMap();
    this._nodes = new Map();
    this._fingerprints = new Map();
  }

  idFor(node) {
    let id = this._ids.get(node);
    if (!id) {
      id = `e${this._next++}`;
      this._ids.set(node, id);
    }
    this._nodes.set(id, new WeakRef(node));
    return id;
  }

  lookup(id) {
    const key = normalizeRefId(id);
    const ref = this._nodes.get(key);
    if (!ref) {
      // Issued before and swept since → gone; never issued → unknown.
      return Number(key.slice(1)) < this._next ? { error: 'gone' } : { error: 'unknown' };
    }
    const node = ref.deref();
    return node ? { node } : { error: 'gone' };
  }

  remember(id, fingerprint) {
    this._fingerprints.set(normalizeRefId(id), fingerprint);
  }

  fingerprintOf(id) {
    return this._fingerprints.get(normalizeRefId(id)) ?? null;
  }

  /** Drop nodes that are gone (collected, or `isAlive` says detached). */
  sweep(isAlive) {
    for (const [id, ref] of this._nodes) {
      const node = ref.deref();
      if (!node || !isAlive(node)) {
        this._nodes.delete(id);
        this._fingerprints.delete(id);
      }
    }
  }
}

/**
 * Why the element behind an id is not the one the snapshot showed, or null.
 * Compares what the model relied on — the document, what the element is, and
 * the form/dialog/row it sits in. Scroll position and field values are left
 * out: the agent's own typing and scrolling change them on purpose.
 */
export function staleReason(before, now) {
  if (!before) {
    return null;
  }
  if (before.url !== now.url) {
    return 'the page changed';
  }
  if (before.role !== now.role) {
    return `it is now a ${now.role || 'different element'}`;
  }
  if (before.context !== now.context) {
    return `it moved into ${now.context || 'a different part of the page'}`;
  }
  return null;
}

export function refMissingMessage(id, error) {
  return error === 'gone'
    ? `Element ${id} is no longer on the page; nothing was done. Take a new snapshot.`
    : `Element ${id} is not in any snapshot of this page; nothing was done. Take a new snapshot.`;
}

export function staleMessage(id, reason) {
  return `Element ${id} changed since the snapshot (${reason}); nothing was done. Take a new snapshot.`;
}

/**
 * Why `el` cannot be filled with text, or null. A <select> can: its value is
 * chosen by option label or value (selectPlan). Toggles and buttons are
 * clicked, never given a `.value` behind the page's back.
 */
export function fillTargetProblem({ tagName, type, isContentEditable, role }) {
  if (String(tagName).toUpperCase() === 'SELECT') {
    return null;
  }
  if (isTextEditable({ tagName, type, isContentEditable })) {
    return null;
  }
  const kind = role || String(type || tagName || 'element').toLowerCase();
  if (
    ['checkbox', 'radio', 'button', 'switch', 'link', 'option', 'tab', 'menuitem'].includes(kind)
  ) {
    return `This element is a ${kind}; click it instead of filling it.`;
  }
  return `This element (${kind}) is not a text field; nothing was typed.`;
}

/** Which option of a select to choose for `wanted` (label or value). */
export function selectPlan(options, wanted) {
  const w = String(wanted).trim().toLowerCase();
  const index = options.findIndex(
    (o) => o.label.trim().toLowerCase() === w || String(o.value).toLowerCase() === w
  );
  if (index === -1) {
    const names = options
      .slice(0, 25)
      .map((o) => o.label)
      .join(', ');
    return {
      error: `No option "${wanted}". Options: ${names}${options.length > 25 ? ', …' : ''}`,
    };
  }
  if (options[index].disabled) {
    return { error: `Option "${options[index].label}" is disabled.` };
  }
  return { index };
}

/** How long to wait after a click before the next observation (§4.5 等待). */
export function waitAfterClick({ role, hasPopup, expanded }) {
  const opensList =
    (role === 'combobox' && expanded !== 'true') ||
    ['listbox', 'menu', 'true'].includes(String(hasPopup));
  return opensList ? { forOptions: true, maxMs: 200 } : { frames: 2, maxMs: 50 };
}

/** The viewport's visible text for the snapshot, bounded (§4.5 快照). */
export function capVisibleText(chunks, cap = 1500) {
  const seen = new Set();
  const lines = [];
  for (const c of chunks) {
    const t = String(c).replace(/\s+/g, ' ').trim();
    if (t && !seen.has(t)) {
      seen.add(t);
      lines.push(t);
    }
  }
  const all = lines.join('\n');
  return all.length > cap ? `${all.slice(0, cap)} …(+${all.length - cap} chars)` : all;
}

/** Which options of a select the snapshot lists: the first `cap`, plus the selected one. */
export function selectOptionEntries(options, cap = 25) {
  const shown = options.slice(0, cap).map((_, i) => i);
  const sel = options.findIndex((o) => o.selected);
  if (sel >= cap) {
    shown.push(sel);
  }
  return { shown, more: options.length - shown.length };
}

/**
 * The element kind the staleness fingerprint compares, from the DOM only.
 * Gecko builds accessibles lazily, so an a11y role can appear between the
 * snapshot and the action; a fingerprint using it saw a change that wasn't.
 */
export function domRoleKey({ tagName, roleAttr, type }) {
  if (roleAttr) {
    return String(roleAttr).trim().toLowerCase();
  }
  const tag = String(tagName || '').toLowerCase();
  return tag === 'input' ? `input:${String(type || 'text').toLowerCase()}` : tag;
}

/** Snapshot entries with one entry per node (the first wins). */
export function uniqueByNode(entries) {
  const seen = new Set();
  return entries.filter((e) => {
    if (seen.has(e.node)) {
      return false;
    }
    seen.add(e.node);
    return true;
  });
}

/**
 * Walk the a11y tree again after a short wait? Gecko builds a page's
 * accessible tree lazily, so the first walk of a fresh page finds nothing
 * and every control would print as `?tag`.
 */
export function shouldRetryA11yWalk({ a11yCount, hasBody, retried }) {
  return a11yCount === 0 && hasBody && !retried;
}

/**
 * Whether the node behind an id can still be acted on: connected, and its
 * document still shown. A node of an iframe document that navigated away
 * stays isConnected (its root is the old Document) but has no window.
 */
export function refNodeIsLive(node) {
  return Boolean(node && node.isConnected && node.ownerDocument?.defaultView);
}

/** Count `role|name` pairs of a page's accessibles (one walk per snapshot). */
export function tallyRoleNames(pairs) {
  const tally = new Map();
  for (const [role, name] of pairs) {
    const key = `${role}|${name}`;
    tally.set(key, (tally.get(key) || 0) + 1);
  }
  return tally;
}

/** Whether exactly one accessible has this role and name. */
export function isUniqueRoleName(tally, role, name) {
  return tally.get(`${role}|${name}`) === 1;
}

/** Whether `node` is a listed element or inside one (parent walk, Set lookup). */
export function withinListed(node, listedSet) {
  for (let n = node; n; n = n.parentElement) {
    if (listedSet.has(n)) {
      return true;
    }
  }
  return false;
}
