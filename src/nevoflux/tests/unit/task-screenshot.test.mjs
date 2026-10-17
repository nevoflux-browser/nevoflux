/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { readFileSync } from 'node:fs';
import { describe, it, expect } from './test-runner.mjs';
import {
  SYSTEM_ORIGIN,
  TASK_SCREENSHOT_LIMITS,
  isSystemRequest,
  systemRequestRefusal,
  systemOriginRefusal,
  withoutOrigin,
  taskScreenshotOptions,
  base64Bytes,
  screenshotError,
  captureWithinBudget,
  canvasTabForArtifact,
} from '../../extensions/nevoflux-agent/background/task-screenshot.mjs';

const shotRequest = (over = {}) => ({
  request_id: 'r1',
  session_id: 's1',
  origin: 'system',
  action: 'screenshot',
  tab_id: null,
  timeout_ms: 30000,
  params: {
    purpose: 'task_result',
    artifact_id: 'a1',
    format: 'jpeg',
    quality: 60,
    retry_quality: 40,
    max_width: 1280,
    max_height: 2400,
    max_bytes: 307200,
  },
  ...over,
});

// base64 of n bytes (n a multiple of 3, so no padding).
const b64Of = (bytes) => 'A'.repeat((bytes / 3) * 4);

describe('system requests', () => {
  it('only origin "system" is a system request', () => {
    expect(SYSTEM_ORIGIN).toBe('system');
    expect(isSystemRequest(shotRequest())).toBe(true);
    for (const r of [{ origin: 'agent' }, { origin: 'System' }, {}, null, undefined]) {
      expect(isSystemRequest(r)).toBe(false);
    }
  });

  it('the task screenshot is the only system request', () => {
    expect(systemRequestRefusal(shotRequest())).toBeNull();
    for (const bad of [
      shotRequest({ action: 'eval_js' }),
      shotRequest({ action: 'navigate' }),
      shotRequest({ params: { purpose: 'debug' } }),
      shotRequest({ params: undefined }),
    ]) {
      const refusal = systemRequestRefusal(bad);
      expect(refusal.code).toBe(-1);
      expect(refusal.recoverable).toBe(false);
    }
    expect(systemOriginRefusal('screenshot').recoverable).toBe(false);
  });

  it('a bridge request cannot carry an origin', () => {
    const smuggled = { action: 'screenshot', origin: 'system', params: { purpose: 'task_result' }, artifact_id: 'x', tab_id: 3 };
    const clean = withoutOrigin(smuggled);
    expect('origin' in clean).toBe(false);
    expect(isSystemRequest(clean)).toBe(false);
    expect(clean).toEqual({ action: 'screenshot', params: { purpose: 'task_result' }, artifact_id: 'x', tab_id: 3 });
    expect('origin' in withoutOrigin({ action: 'click', origin: 'agent' })).toBe(false);
    expect(smuggled.origin).toBe('system'); // the caller's object is untouched
    expect(withoutOrigin(null)).toBeNull();
  });
});

describe('taskScreenshotOptions', () => {
  it('reads the contract params', () => {
    expect(taskScreenshotOptions(shotRequest().params)).toEqual({
      quality: 60,
      retryQuality: 40,
      maxWidth: 1280,
      maxHeight: 2400,
      maxBytes: 307200,
    });
  });

  it('a request may lower the limits but never raise them', () => {
    const o = taskScreenshotOptions({ quality: 95, retry_quality: 90, max_width: 4000, max_height: 9000, max_bytes: 5_000_000 });
    expect(o).toEqual(taskScreenshotOptions({}));
    expect(o.maxBytes).toBe(TASK_SCREENSHOT_LIMITS.maxBytes);
    expect(taskScreenshotOptions({ max_width: 640, quality: 50 }).maxWidth).toBe(640);
    expect(taskScreenshotOptions({ max_width: -5, quality: '60' }).maxWidth).toBe(1280);
    expect(taskScreenshotOptions(undefined).quality).toBe(60);
  });
});

describe('base64Bytes and screenshotError', () => {
  it('counts decoded bytes, padding included', () => {
    expect(base64Bytes('QUJD')).toBe(3);
    expect(base64Bytes('QUI=')).toBe(2);
    expect(base64Bytes('QQ==')).toBe(1);
    expect(base64Bytes('')).toBe(0);
    expect(base64Bytes(undefined)).toBe(0);
  });

  it('passes the four contract errors through and folds the rest into capture_failed', () => {
    for (const m of ['tab_discarded', 'tab_closed', 'too_large', 'capture_failed']) {
      expect(screenshotError({ message: m })).toBe(m);
    }
    expect(screenshotError({ message: 'Tab not found' })).toBe('capture_failed');
    expect(screenshotError(undefined)).toBe('capture_failed');
  });
});

describe('captureWithinBudget', () => {
  const opts = taskScreenshotOptions({});
  const shot = (bytes, extra = {}) => ({ success: true, data: b64Of(bytes), width: 1280, height: 720, tabKind: 'canvas', ...extra });

  it('a capture that fits at q60 is sent once, in the contract shape', async () => {
    const asked = [];
    const res = await captureWithinBudget(async (q) => {
      asked.push(q);
      return shot(300_000);
    }, opts);
    expect(asked).toEqual([60]);
    expect(res).toEqual({
      success: true,
      result: { mime_type: 'image/jpeg', data: b64Of(300_000), width: 1280, height: 720, bytes: 300_000, tab_kind: 'canvas' },
    });
  });

  it('a capture over budget is retried once at q40', async () => {
    const asked = [];
    const res = await captureWithinBudget(async (q) => {
      asked.push(q);
      return q === 60 ? shot(400_002) : shot(200_001, { tabKind: 'web' });
    }, opts);
    expect(asked).toEqual([60, 40]);
    expect(res.success).toBe(true);
    expect(res.result.tab_kind).toBe('web');
    expect(res.result.bytes).toBe(200_001);
  });

  it('still over budget at q40 is too_large', async () => {
    const res = await captureWithinBudget(async () => shot(400_002), opts);
    expect(res).toEqual({ success: false, error: { code: -1, message: 'too_large', recoverable: false } });
  });

  it('a failed capture reports the contract error and is not retried', async () => {
    let calls = 0;
    const res = await captureWithinBudget(async () => {
      calls++;
      return { success: false, error: { code: -1, message: 'tab_discarded' } };
    }, opts);
    expect(calls).toBe(1);
    expect(res.error.message).toBe('tab_discarded');
  });

  it('a throw, an empty image or a data: URL never leaks out as a success', async () => {
    const thrown = await captureWithinBudget(async () => {
      throw new Error('boom');
    }, opts);
    expect(thrown.error.message).toBe('capture_failed');
    const empty = await captureWithinBudget(async () => ({ success: true, data: '', width: 1, height: 1 }), opts);
    expect(empty.error.message).toBe('capture_failed');
    const prefixed = await captureWithinBudget(
      async () => ({ success: true, data: 'data:image/jpeg;base64,' + b64Of(3), width: 1, height: 1, tabKind: 'web' }),
      opts
    );
    expect(prefixed.result.data).toBe(b64Of(3));
  });
});

describe('canvasTabForArtifact', () => {
  it('finds the tab showing nevoflux://canvas/<id>, exactly', () => {
    const tabs = [
      { id: 3, url: 'https://example.com/' },
      { id: 4, url: 'nevoflux://canvas/a10' },
      { id: 5, url: 'nevoflux://canvas/a1?mode=preview' },
    ];
    expect(canvasTabForArtifact(tabs, 'a1')).toBe(5);
    expect(canvasTabForArtifact(tabs, 'a10')).toBe(4);
    expect(canvasTabForArtifact(tabs, 'zz')).toBeNull();
    expect(canvasTabForArtifact(tabs, '')).toBeNull();
    expect(canvasTabForArtifact(null, 'a1')).toBeNull();
  });
});

const read = (rel) => readFileSync(new URL(rel, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const PARENT = '../../engine-overlays/browser/components/extensions/parent/ext-nevoflux.js';
const SCHEMA = '../../engine-overlays/browser/components/extensions/schemas/nevoflux.json';
const CHILD = '../../engine-overlays/browser/actors/NevofluxChild.sys.mjs';

// The real method out of ext-nevoflux.js, run against fakes. The file is a
// privileged parent script Node cannot load whole; this method only touches
// `extension`, `self` and (on the Canvas path, not exercised here) ChromeUtils.
function loadCaptureTaskScreenshot(extension, self) {
  const src = read(PARENT);
  const start = src.indexOf('        async captureTaskScreenshot(tabId, options) {');
  if (start < 0) {
    throw new Error('captureTaskScreenshot not found in ext-nevoflux.js');
  }
  const end = src.indexOf('\n        },\n', start);
  const method = src.slice(start, end + '\n        }'.length);
  // eslint-disable-next-line no-new-func
  return new Function('extension', 'self', 'ChromeUtils', `return ({\n${method}\n}).captureTaskScreenshot;`)(
    extension,
    self,
    {}
  );
}

function fakeTab(url, extra = {}) {
  return { closing: false, isConnected: true, linkedBrowser: { currentURI: { spec: url } }, ...extra };
}

function fakeWorld(tabs, childResult) {
  const calls = { execute: [], restored: 0 };
  const extension = {
    tabManager: {
      get(id) {
        if (!tabs.has(id)) throw new Error(`Invalid tab ID: ${id}`);
        return { nativeTab: tabs.get(id) };
      },
    },
  };
  const self = {
    isTabDiscarded: (t) => t.discarded === true,
    restoreTabIfNeeded: async () => {
      calls.restored++;
      return true;
    },
    executeInTabWithRestore: async (tabId, ext, action, params, options) => {
      calls.execute.push({ tabId, action, params, options });
      return childResult;
    },
  };
  return { capture: loadCaptureTaskScreenshot(extension, self), calls };
}

describe('captureTaskScreenshot experiment API', () => {
  it('the schema declares captureTaskScreenshot(tabId: integer, options?: object)', () => {
    const ns = JSON.parse(read(SCHEMA)).find((n) => n.namespace === 'nevoflux');
    const fn = ns.functions.find((f) => f.name === 'captureTaskScreenshot');
    expect(fn === undefined).toBe(false);
    expect(fn.async).toBe('promise');
    expect(fn.parameters.map((p) => `${p.name}:${p.type}:${p.optional === true}`)).toEqual([
      'tabId:integer:false',
      'options:object:true',
    ]);
    expect(Object.keys(fn.parameters[1].properties)).toEqual(['quality', 'maxWidth', 'maxHeight']);
  });

  it('an unknown, closing or detached tab is tab_closed', async () => {
    const { capture, calls } = fakeWorld(
      new Map([
        [8, fakeTab('https://a.example/', { closing: true })],
        [9, fakeTab('https://b.example/', { isConnected: false })],
      ]),
      { success: true }
    );
    for (const id of [99, 8, 9]) {
      expect(await capture(id, {})).toEqual({
        success: false,
        error: { code: -1, message: 'tab_closed', recoverable: false },
      });
    }
    expect(calls.execute.length).toBe(0);
  });

  it('a discarded tab is reported, never restored or selected', async () => {
    const { capture, calls } = fakeWorld(
      new Map([[5, fakeTab('https://a.example/', { discarded: true })]]),
      { success: true, data: 'QUJD', width: 1, height: 1 }
    );
    expect(await capture(5, { quality: 60 })).toEqual({
      success: false,
      error: { code: -1, message: 'tab_discarded', recoverable: false },
    });
    expect(calls.execute.length).toBe(0);
    expect(calls.restored).toBe(0);
  });

  it('a web tab is drawn by the child, viewport only, with autoRestore off', async () => {
    const { capture, calls } = fakeWorld(
      new Map([[5, fakeTab('https://a.example/')]]),
      { success: true, data: 'QUJD', width: 1280, height: 700, mimeType: 'image/jpeg' }
    );
    const res = await capture(5, { quality: 40, maxWidth: 1280, maxHeight: 2400 });
    expect(res).toEqual({ success: true, data: 'QUJD', width: 1280, height: 700, tabKind: 'web' });
    expect(calls.execute).toEqual([
      {
        tabId: 5,
        action: 'screenshot',
        params: { fullPage: false, type: 'jpeg', quality: 40, maxWidth: 1280, maxHeight: 2400 },
        options: { autoRestore: false },
      },
    ]);
  });

  it('the child skipping a discarded tab (5008) is tab_discarded; anything else capture_failed', async () => {
    const skipped = fakeWorld(new Map([[5, fakeTab('https://a.example/')]]), {
      success: false,
      error: { code: 5008, message: 'Tab is discarded; skipped to preserve focus' },
    });
    expect((await skipped.capture(5, {})).error).toEqual({ code: -1, message: 'tab_discarded', recoverable: false });
    const broken = fakeWorld(new Map([[5, fakeTab('https://a.example/')]]), {
      success: false,
      error: { code: 5003, message: 'Screenshot failed: x' },
    });
    expect((await broken.capture(5, {})).error).toEqual({ code: -1, message: 'capture_failed', recoverable: false });
  });

  it('the child crops to maxHeight, keeping the top', () => {
    const child = read(CHILD);
    expect(
      child.includes("async screenshot({ fullPage = false, type = 'jpeg', quality = 60, maxWidth = 1280, maxHeight = 0 }) {")
    ).toBe(true);
    expect(child.includes('if (maxHeight > 0 && outputHeight > maxHeight) {')).toBe(true);
  });
});
