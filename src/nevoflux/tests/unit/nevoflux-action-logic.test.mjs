/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Imports the real module the actor imports — not a copy.
import { describe, it, expect } from './test-runner.mjs';
import {
  snapshotValue,
  EFFECT_ATTRIBUTES,
  isMeaningfulAttributeChange,
  isTextEditable,
  controlStateChanged,
  pointInViewport,
  classifyClickPoints,
  shouldTryNextTier,
  describeOccluder,
  coveredMessage,
  clickEffect,
  shouldRescrollAndRepick,
  canonicalRole,
  isInteractiveRole,
  RefRegistry,
  staleReason,
  refMissingMessage,
  staleMessage,
  fillTargetProblem,
  selectPlan,
  selectOptionEntries,
  waitAfterClick,
  capVisibleText,
} from '../../engine-overlays/browser/actors/NevofluxActionLogic.sys.mjs';

describe('snapshotValue', () => {
  it('never shows a password, file or hidden input value', () => {
    for (const type of ['password', 'PASSWORD', 'file', 'hidden']) {
      expect(snapshotValue({ tagName: 'INPUT', type, value: 'hunter2' })).toBeNull();
    }
  });

  it('shows other values, cut to 30 characters', () => {
    expect(snapshotValue({ tagName: 'INPUT', type: 'text', value: 'Alice' })).toBe('Alice');
    expect(snapshotValue({ tagName: 'SELECT', type: 'select-one', value: 'XL' })).toBe('XL');
    const long = 'x'.repeat(40);
    expect(snapshotValue({ tagName: 'TEXTAREA', type: 'textarea', value: long })).toBe(
      'x'.repeat(27) + '...'
    );
  });

  it('shows nothing for an empty or missing value', () => {
    expect(snapshotValue({ tagName: 'INPUT', type: 'text', value: '' })).toBeNull();
    expect(snapshotValue({ tagName: 'DIV', type: undefined, value: undefined })).toBeNull();
  });
});

describe('isMeaningfulAttributeChange', () => {
  it('watches the state attributes a toggle or disclosure changes', () => {
    for (const a of ['aria-checked', 'aria-pressed', 'aria-expanded', 'open', 'hidden']) {
      expect(EFFECT_ATTRIBUTES).toContain(a);
    }
  });

  it('ignores style and hover-only class changes', () => {
    expect(isMeaningfulAttributeChange('style', 'a', 'b')).toBe(false);
    expect(isMeaningfulAttributeChange('class', 'btn', 'btn hover')).toBe(false);
    expect(isMeaningfulAttributeChange('class', 'btn  focused', 'btn')).toBe(false);
  });

  it('counts a real class or state change', () => {
    expect(isMeaningfulAttributeChange('class', 'tab', 'tab selected')).toBe(true);
    expect(isMeaningfulAttributeChange('aria-checked', 'false', 'true')).toBe(true);
    expect(isMeaningfulAttributeChange('open', null, '')).toBe(true);
  });

  it('does not count an attribute set to the value it already had', () => {
    expect(isMeaningfulAttributeChange('aria-expanded', 'true', 'true')).toBe(false);
  });
});

describe('isTextEditable', () => {
  it('is true for text fields and contenteditable', () => {
    expect(isTextEditable({ tagName: 'INPUT', type: 'text' })).toBe(true);
    expect(isTextEditable({ tagName: 'INPUT', type: 'email' })).toBe(true);
    expect(isTextEditable({ tagName: 'TEXTAREA' })).toBe(true);
    expect(isTextEditable({ tagName: 'DIV', isContentEditable: true })).toBe(true);
  });

  it('is false for buttons, checkboxes and plain elements', () => {
    for (const type of ['checkbox', 'radio', 'button', 'submit', 'file', 'range']) {
      expect(isTextEditable({ tagName: 'INPUT', type })).toBe(false);
    }
    expect(isTextEditable({ tagName: 'BUTTON' })).toBe(false);
    expect(isTextEditable({ tagName: 'DIV', isContentEditable: false })).toBe(false);
  });
});

describe('controlStateChanged', () => {
  const base = {
    checked: false,
    value: 'on',
    selectedIndex: null,
    open: null,
    ariaChecked: null,
    ariaPressed: null,
    ariaExpanded: null,
    focused: false,
  };

  it('sees a checkbox tick, a select change and a focused field', () => {
    expect(controlStateChanged(base, { ...base, checked: true })).toBe(true);
    expect(controlStateChanged({ ...base, selectedIndex: 1 }, { ...base, selectedIndex: 3 })).toBe(
      true
    );
    expect(controlStateChanged(base, { ...base, focused: true })).toBe(true);
    expect(controlStateChanged(base, { ...base, ariaPressed: 'true' })).toBe(true);
  });

  it('is false when nothing moved or a state is missing', () => {
    expect(controlStateChanged(base, { ...base })).toBe(false);
    expect(controlStateChanged(null, base)).toBe(false);
    expect(controlStateChanged(base, null)).toBe(false);
  });
});

describe('pointInViewport', () => {
  it('is true inside, false on or past the edges', () => {
    expect(pointInViewport({ x: 10, y: 10 }, 800, 600)).toBe(true);
    expect(pointInViewport({ x: -1, y: 10 }, 800, 600)).toBe(false);
    expect(pointInViewport({ x: 10, y: 600 }, 800, 600)).toBe(false);
    expect(pointInViewport({ x: 800, y: 10 }, 800, 600)).toBe(false);
  });
});

describe('classifyClickPoints', () => {
  it('takes the first point that hits the target', () => {
    expect(classifyClickPoints([{ occluder: 'div#x' }, 'target', 'target'])).toEqual({
      kind: 'target',
      index: 1,
    });
  });

  it('reports the cover when no point hits the target', () => {
    expect(classifyClickPoints(['offscreen', { occluder: 'div#cookie-banner' }])).toEqual({
      kind: 'covered',
      occluder: 'div#cookie-banner',
    });
  });

  it('prefers a point on the target over one on an ancestor', () => {
    // A wrapped inline link: its box centre falls on the paragraph.
    expect(classifyClickPoints(['ancestor', 'offscreen', 'target'])).toEqual({
      kind: 'target',
      index: 2,
    });
  });

  it('uses an ancestor point only when no point hits the target', () => {
    // e.g. a visually hidden checkbox whose label receives the hit.
    expect(classifyClickPoints([{ occluder: 'div#x' }, 'ancestor'])).toEqual({
      kind: 'target',
      index: 1,
    });
  });

  it('is offscreen when every point is outside the viewport', () => {
    expect(classifyClickPoints(['offscreen', 'offscreen'])).toEqual({ kind: 'offscreen' });
    expect(classifyClickPoints([])).toEqual({ kind: 'offscreen' });
  });
});

describe('shouldTryNextTier', () => {
  it('falls back only when nothing was pressed', () => {
    expect(shouldTryNextTier('unavailable')).toBe(true);
    expect(shouldTryNextTier('threw_before_press')).toBe(true);
    expect(shouldTryNextTier('pressed')).toBe(false);
  });
});

describe('describeOccluder / coveredMessage', () => {
  it('names the cover by tag, id, role and label', () => {
    expect(
      describeOccluder({
        tagName: 'DIV',
        id: 'cookie-banner',
        role: 'dialog',
        label: '  We use\n cookies. ',
      })
    ).toBe('div#cookie-banner[role=dialog] "We use cookies."');
    expect(describeOccluder({ tagName: 'SPAN' })).toBe('span');
    expect(describeOccluder({ tagName: 'DIV', label: 'y'.repeat(60) })).toBe(
      `div "${'y'.repeat(37)}..."`
    );
  });

  it('says the click was not sent and what to do', () => {
    const m = coveredMessage('div#cookie-banner');
    expect(m).toContain('covered by div#cookie-banner');
    expect(m).toContain('not sent');
    expect(m).toContain('new snapshot');
  });
});

describe('clickEffect', () => {
  it('observed when something changed or the element went away', () => {
    expect(clickEffect({ method: 'trusted_event', changed: true, elementRemoved: false })).toEqual({
      effective: true,
      effect: 'observed',
    });
    expect(
      clickEffect({ method: 'native_click', changed: false, elementRemoved: true }).effect
    ).toBe('observed');
  });

  it('none_observed when a sent click changed nothing', () => {
    expect(
      clickEffect({ method: 'trusted_event', changed: false, elementRemoved: false })
    ).toEqual({ effective: false, effect: 'none_observed' });
  });

  it('not_dispatched when no tier could send', () => {
    expect(
      clickEffect({ method: 'all_tiers_exhausted', changed: false, elementRemoved: false })
    ).toEqual({ effective: false, effect: 'not_dispatched' });
  });
});

describe('shouldRescrollAndRepick', () => {
  it('scrolls the target into view once when no point hit it', () => {
    // A target clipped by an inner scroll container is inside the window
    // viewport, but its points land outside the scroller.
    expect(shouldRescrollAndRepick({ kind: 'covered', occluder: 'div' }, false)).toBe(true);
    expect(shouldRescrollAndRepick({ kind: 'offscreen' }, false)).toBe(true);
  });

  it('not when a point hit it, nor a second time', () => {
    expect(shouldRescrollAndRepick({ kind: 'target', index: 0 }, false)).toBe(false);
    expect(shouldRescrollAndRepick({ kind: 'covered', occluder: 'div' }, true)).toBe(false);
  });
});

describe('canonicalRole', () => {
  it('maps Gecko role names (RoleMap.inc) to ARIA roles', () => {
    const cases = {
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
    for (const [gecko, aria] of Object.entries(cases)) {
      expect(canonicalRole(gecko)).toBe(aria);
    }
  });

  it('gives no role to containers and text', () => {
    for (const gecko of ['text container', 'section', 'paragraph', 'text leaf', 'landmark', '']) {
      expect(canonicalRole(gecko)).toBe('');
    }
  });

  it('marks exactly the canonical control roles interactive', () => {
    expect(isInteractiveRole('button')).toBe(true);
    expect(isInteractiveRole('option')).toBe(true);
    expect(isInteractiveRole('')).toBe(false);
    expect(isInteractiveRole('pushbutton')).toBe(false); // Gecko name, not canonical
  });
});

describe('RefRegistry', () => {
  it('gives a node the same id in every snapshot, and new nodes new ids', () => {
    const reg = new RefRegistry();
    const a = { n: 'a' };
    const b = { n: 'b' };
    expect(reg.idFor(a)).toBe('e0');
    expect(reg.idFor(b)).toBe('e1');
    expect(reg.idFor(a)).toBe('e0');
  });

  it('resolves ids in any spelling to the node', () => {
    const reg = new RefRegistry();
    const a = {};
    reg.idFor(a);
    for (const id of ['e0', '0', 0]) {
      expect(reg.lookup(id).node).toBe(a);
    }
  });

  it('says unknown for ids it never gave, gone for swept nodes', () => {
    const reg = new RefRegistry();
    const a = {};
    reg.idFor(a);
    expect(reg.lookup('e7')).toEqual({ error: 'unknown' });
    reg.sweep(() => false);
    expect(reg.lookup('e0')).toEqual({ error: 'gone' });
  });

  it('keeps the fingerprint remembered for an id', () => {
    const reg = new RefRegistry();
    const id = reg.idFor({});
    reg.remember(id, { url: 'u', role: 'button', context: '' });
    expect(reg.fingerprintOf(id)).toEqual({ url: 'u', role: 'button', context: '' });
    expect(reg.fingerprintOf('e99')).toBeNull();
  });
});

describe('staleReason', () => {
  const fp = { url: 'https://a/x', role: 'button', context: 'form#checkout' };

  it('is null when url, role and context are unchanged', () => {
    expect(staleReason(fp, { ...fp })).toBeNull();
    expect(staleReason(null, fp)).toBeNull(); // nothing remembered: no check
  });

  it('names what changed', () => {
    expect(staleReason(fp, { ...fp, url: 'https://a/y' })).toContain('page changed');
    expect(staleReason(fp, { ...fp, role: 'link' })).toContain('now a link');
    expect(staleReason(fp, { ...fp, context: 'dialog#confirm' })).toContain('dialog#confirm');
  });

  it('messages tell the model to take a new snapshot', () => {
    expect(refMissingMessage('e3', 'gone')).toContain('no longer on the page');
    expect(refMissingMessage('e3', 'unknown')).toContain('new snapshot');
    expect(staleMessage('e3', 'the page changed')).toContain('new snapshot');
  });
});

import { readFileSync } from 'node:fs';

describe('actor source', () => {
  const src = readFileSync(
    new URL('../../engine-overlays/browser/actors/NevofluxChild.sys.mjs', import.meta.url),
    'utf8'
  );
  it('never writes data-ai-id into the page', () => {
    expect(src.includes("setAttribute('data-ai-id'")).toBe(false);
  });
});

describe('fillTargetProblem', () => {
  it('allows text fields, contenteditable and selects', () => {
    expect(fillTargetProblem({ tagName: 'INPUT', type: 'email' })).toBeNull();
    expect(fillTargetProblem({ tagName: 'DIV', isContentEditable: true })).toBeNull();
    expect(fillTargetProblem({ tagName: 'SELECT' })).toBeNull();
  });

  it('refuses checkboxes, radios and buttons with what to do instead', () => {
    expect(fillTargetProblem({ tagName: 'INPUT', type: 'checkbox', role: 'checkbox' })).toContain(
      'click it'
    );
    expect(fillTargetProblem({ tagName: 'BUTTON', role: 'button' })).toContain('click it');
    expect(fillTargetProblem({ tagName: 'DIV', role: '' })).toContain('not a text field');
  });
});

describe('selectPlan', () => {
  const opts = [
    { label: 'Small', value: 'S', disabled: false },
    { label: 'Extra large', value: 'XL', disabled: false },
    { label: 'Gone', value: 'G', disabled: true },
  ];
  it('matches label or value, case-insensitively', () => {
    expect(selectPlan(opts, 'extra large')).toEqual({ index: 1 });
    expect(selectPlan(opts, 'S')).toEqual({ index: 0 });
  });
  it('refuses a disabled or missing option and lists what exists', () => {
    expect(selectPlan(opts, 'Gone').error).toContain('disabled');
    const miss = selectPlan(opts, 'Medium').error;
    expect(miss).toContain('Small');
    expect(miss).toContain('Extra large');
  });
});

describe('selectOptionEntries', () => {
  const mk = (n, selected = -1) =>
    Array.from({ length: n }, (_, i) => ({
      label: `o${i}`,
      selected: i === selected,
      disabled: false,
    }));

  it('lists all options of a short select', () => {
    expect(selectOptionEntries(mk(4))).toEqual({ shown: [0, 1, 2, 3], more: 0 });
  });

  it('caps a long select and keeps the selected option in view', () => {
    const r = selectOptionEntries(mk(300, 250), 25);
    expect(r.shown.length).toBe(26);
    expect(r.shown[25]).toBe(250);
    expect(r.more).toBe(274);
  });
});
