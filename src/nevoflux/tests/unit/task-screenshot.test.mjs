/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { readFileSync } from 'node:fs';
import { describe, it, expect } from './test-runner.mjs';
import * as ActionLogic from '../../engine-overlays/browser/actors/NevofluxActionLogic.sys.mjs';
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
// `extension`, `self` and, on the Canvas path, two modules through
// ChromeUtils: Timer (made instant here) and the real NevofluxActionLogic.
const FAKE_CHROME_UTILS = {
  importESModule(url) {
    if (url === 'resource://gre/modules/Timer.sys.mjs') {
      return { setTimeout: (fn) => setTimeout(fn, 0) };
    }
    if (url === 'resource:///actors/NevofluxActionLogic.sys.mjs') {
      return ActionLogic;
    }
    throw new Error(`unexpected module ${url}`);
  },
};

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
    FAKE_CHROME_UTILS
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

describe('fitTop (task screenshot size math)', () => {
  const { fitTop } = ActionLogic;

  it('never upscales a frame that already fits', () => {
    expect(fitTop(800, 600, 1280, 2400)).toEqual({ width: 800, height: 600, srcHeight: 600 });
  });

  it('scales a wide frame down to maxWidth, keeping the aspect ratio', () => {
    expect(fitTop(2560, 1440, 1280, 2400)).toEqual({ width: 1280, height: 720, srcHeight: 1440 });
  });

  it('keeps the top segment of a tall frame', () => {
    expect(fitTop(1000, 5000, 1280, 2400)).toEqual({ width: 1000, height: 2400, srcHeight: 2400 });
    expect(fitTop(2560, 6000, 1280, 2400)).toEqual({ width: 1280, height: 2400, srcHeight: 4800 });
  });

  it('stays within maxWidth x maxHeight and inside the source', () => {
    const sizes = [
      [1, 1],
      [1279, 2401],
      [1281, 2399],
      [3840, 2160],
      [1366, 30000],
      [7, 9999],
      [4096, 4096],
    ];
    for (const [w, h] of sizes) {
      const r = fitTop(w, h, 1280, 2400);
      expect(r.width <= 1280 && r.height <= 2400).toBe(true);
      expect(r.width >= 1 && r.height >= 1).toBe(true);
      expect(r.srcHeight >= 1 && r.srcHeight <= h).toBe(true);
      expect(r.width <= w).toBe(true);
      // Same scale on both axes (within rounding).
      expect(Math.abs(r.width / w - r.height / r.srcHeight) < 0.01).toBe(true);
    }
  });

  it('handles tiny limits and rejects zero or invalid input', () => {
    expect(fitTop(1, 1, 1280, 2400)).toEqual({ width: 1, height: 1, srcHeight: 1 });
    expect(fitTop(3, 10000, 1, 1)).toEqual({ width: 1, height: 1, srcHeight: 3 });
    expect(fitTop(0, 600, 1280, 2400)).toBeNull();
    expect(fitTop(800, 0, 1280, 2400)).toBeNull();
    expect(fitTop(800, 600, 0, 2400)).toBeNull();
    expect(fitTop(800, 600, 1280, -1)).toBeNull();
    expect(fitTop(Number.NaN, 600, 1280, 2400)).toBeNull();
    expect(fitTop(undefined, undefined, 1280, 2400)).toBeNull();
  });
});

function fakeBitmap(width, height) {
  return {
    width,
    height,
    closed: false,
    close() {
      this.closed = true;
    },
  };
}

function fakeFrame(id, bitmap, log, { wgpAfter = 0 } = {}) {
  let reads = 0;
  return {
    id,
    children: [],
    get currentWindowGlobal() {
      reads++;
      if (reads <= wgpAfter) {
        return null;
      }
      return {
        async drawSnapshot(rect, scale, background) {
          log.snapshots.push({ id, rect, scale, background });
          return bitmap;
        },
      };
    },
  };
}

// A Canvas tab whose top browsing context exposes `frames` as children only
// after `framesAfter` reads (a still-loading page), and whose page actor
// reports `reportedId` (or throws when `reportedId` is an Error).
function canvasWorld({ frames, reportedId, framesAfter = 0 }) {
  const log = { queries: [], snapshots: [], drawImage: [], toDataURL: [] };
  let childReads = 0;
  const topBc = {
    get children() {
      childReads++;
      return childReads <= framesAfter ? [] : frames;
    },
    currentWindowGlobal: {
      getActor(name) {
        return {
          async sendQuery(msg) {
            log.queries.push({ name, msg });
            if (reportedId instanceof Error) {
              throw reportedId;
            }
            return { browsingContextId: reportedId };
          },
        };
      },
    },
  };
  const doc = {
    createElementNS(ns, tag) {
      log.created = { ns, tag };
      const canvas = {
        width: 0,
        height: 0,
        getContext: () => ({ drawImage: (...args) => log.drawImage.push(args) }),
        toDataURL: (type, q) => {
          log.toDataURL.push({ type, q, width: canvas.width, height: canvas.height });
          return 'data:image/jpeg;base64,QkFTRTY0';
        },
      };
      return canvas;
    },
  };
  const tab = {
    closing: false,
    isConnected: true,
    ownerGlobal: { document: doc },
    linkedBrowser: { currentURI: { spec: 'nevoflux://canvas/a1' }, browsingContext: topBc },
  };
  const world = fakeWorld(new Map([[7, tab]]), { success: false });
  return { ...world, log };
}

describe('captureTaskScreenshot Canvas path', () => {
  it('draws the frame the page reports, scaled and top-cropped to the limits', async () => {
    const bitmap = fakeBitmap(2560, 6000);
    const log0 = { snapshots: [] };
    const decoy = fakeFrame(11, fakeBitmap(10, 10), log0);
    const preview = fakeFrame(12, bitmap, log0);
    const { capture, calls, log } = canvasWorld({ frames: [decoy, preview], reportedId: 12 });
    const res = await capture(7, { quality: 40, maxWidth: 1280, maxHeight: 2400 });
    expect(res).toEqual({ success: true, data: 'QkFTRTY0', width: 1280, height: 2400, tabKind: 'canvas' });
    expect(log.queries[0]).toEqual({ name: 'Nevoflux', msg: 'canvas:previewFrame' });
    expect(log0.snapshots).toEqual([{ id: 12, rect: null, scale: 1, background: 'rgb(255,255,255)' }]);
    expect(log.drawImage).toEqual([[bitmap, 0, 0, 2560, 4800, 0, 0, 1280, 2400]]);
    expect(log.toDataURL).toEqual([{ type: 'image/jpeg', q: 0.4, width: 1280, height: 2400 }]);
    expect(bitmap.closed).toBe(true);
    expect(calls.execute.length).toBe(0);
  });

  it('finds a reported frame nested below the top level', async () => {
    const log0 = { snapshots: [] };
    const inner = fakeFrame(21, fakeBitmap(800, 600), log0);
    const outer = fakeFrame(20, fakeBitmap(10, 10), log0);
    outer.children = [inner];
    const { capture } = canvasWorld({ frames: [outer], reportedId: 21 });
    const res = await capture(7, {});
    expect(res.success).toBe(true);
    expect(res.width).toBe(800);
    expect(log0.snapshots.map((s) => s.id)).toEqual([21]);
  });

  it('waits for a reported frame that is not attached yet instead of drawing another', async () => {
    const log0 = { snapshots: [] };
    const decoy = fakeFrame(61, fakeBitmap(10, 10), log0);
    const preview = fakeFrame(62, fakeBitmap(800, 600), log0);
    const frames = [decoy];
    const { capture } = canvasWorld({ frames, reportedId: 62 });
    setTimeout(() => frames.push(preview), 0);
    const res = await capture(7, {});
    expect(res.width).toBe(800);
    expect(log0.snapshots.map((s) => s.id)).toEqual([62]);
  });

  it('falls back to the first child frame when the page reports nothing usable', async () => {
    for (const reportedId of [new Error('no actor'), null, 99]) {
      const log0 = { snapshots: [] };
      const first = fakeFrame(31, fakeBitmap(800, 600), log0);
      const second = fakeFrame(32, fakeBitmap(800, 600), log0);
      const { capture } = canvasWorld({ frames: [first, second], reportedId });
      const res = await capture(7, {});
      expect(res.success).toBe(true);
      expect(log0.snapshots.map((s) => s.id)).toEqual([31]);
    }
  });

  it('waits for a still-loading page to grow its preview frame', async () => {
    const log0 = { snapshots: [] };
    const preview = fakeFrame(41, fakeBitmap(800, 600), log0, { wgpAfter: 3 });
    const { capture } = canvasWorld({ frames: [preview], reportedId: 41, framesAfter: 5 });
    const res = await capture(7, {});
    expect(res).toEqual({ success: true, data: 'QkFTRTY0', width: 800, height: 600, tabKind: 'canvas' });
  });

  it('gives up with capture_failed when no frame ever appears', async () => {
    const { capture, log } = canvasWorld({ frames: [], reportedId: null });
    expect(await capture(7, {})).toEqual({
      success: false,
      error: { code: -1, message: 'capture_failed', recoverable: false },
    });
    expect(log.drawImage.length).toBe(0);
  });

  it('a frame that never gets a window global, or an empty snapshot, is capture_failed', async () => {
    const log0 = { snapshots: [] };
    const stuck = fakeFrame(51, fakeBitmap(800, 600), log0, { wgpAfter: 1e9 });
    const a = canvasWorld({ frames: [stuck], reportedId: 51 });
    expect((await a.capture(7, {})).error.message).toBe('capture_failed');
    const empty = fakeFrame(52, fakeBitmap(0, 0), log0);
    const b = canvasWorld({ frames: [empty], reportedId: 52 });
    expect((await b.capture(7, {})).error.message).toBe('capture_failed');
  });
});

describe('NevofluxChild reports the Canvas preview frame', () => {
  it('answers canvas:previewFrame on nevoflux:// pages with the #viewport iframe', () => {
    const child = read(CHILD);
    expect(child.includes("if (name === 'canvas:previewFrame' && this._isNevofluxPage()) {")).toBe(true);
    expect(child.includes("doc?.getElementById('viewport')?.querySelector('iframe')")).toBe(true);
  });
});
