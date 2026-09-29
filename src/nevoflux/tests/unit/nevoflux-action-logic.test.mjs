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
