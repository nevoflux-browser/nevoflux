/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { readFileSync } from 'node:fs';
import { describe, it, expect } from './test-runner.mjs';

const ROUTER = new URL(
  '../../engine-overlays/browser/modules/NevofluxBridgeRouter.sys.mjs',
  import.meta.url
);
const ACTOR = new URL('../../engine-overlays/browser/actors/NevofluxParent.sys.mjs', import.meta.url);

// A fresh router per test, on a clock the test moves by hand.
function load() {
  let now = 0;
  let nextId = 1;
  const timers = new Map(); // id → { at, fn }
  const Timer = {
    setTimeout: (fn, ms) => {
      const id = nextId++;
      timers.set(id, { at: now + ms, fn });
      return id;
    },
    clearTimeout: (id) => timers.delete(id),
  };
  const ChromeUtils = { importESModule: () => Timer };
  const source = readFileSync(ROUTER, 'utf8').replace(
    'export const NevofluxBridgeRouter',
    'const NevofluxBridgeRouter'
  );
  const router = new Function('ChromeUtils', `${source}\nreturn NevofluxBridgeRouter;`)(
    ChromeUtils
  );
  const advance = (ms) => {
    now += ms;
    for (const [id, t] of [...timers]) {
      if (t.at <= now) {
        timers.delete(id);
        t.fn();
      }
    }
  };
  return { router, advance, timers };
}

const MINUTE = 60 * 1000;

describe('NevofluxBridgeRouter push channels', () => {
  it('a session that hears nothing for five minutes is dropped (canvas agent:chat)', () => {
    const { router, advance } = load();
    const got = [];
    router.subscribe('session-1', (m) => got.push(m));
    advance(5 * MINUTE + 1);
    expect(router.push('session-1', { n: 1 })).toBe(false);
    expect(got.length).toBe(0);
  });

  it('a persistent EventBus channel still delivers after a quiet hour (the stuck Muse switch)', () => {
    const { router, advance } = load();
    const got = [];
    router.subscribe('muse_1', (m) => got.push(m), { persistent: true });
    advance(60 * MINUTE);
    expect(router.push('muse_1', { n: 1 })).toBe(true);
    advance(60 * MINUTE);
    expect(router.push('muse_1', { n: 2 })).toBe(true);
    expect(got.map((m) => m.n)).toEqual([1, 2]);
  });

  it('a persistent channel arms no idle timer, on subscribe or on push', () => {
    const { router, timers } = load();
    router.subscribe('muse_1', () => {}, { persistent: true });
    router.push('muse_1', {});
    expect(timers.size).toBe(0);
  });

  it('a persistent channel ends when its last subscriber leaves', () => {
    const { router } = load();
    const unsub = router.subscribe('muse_1', () => {}, { persistent: true });
    unsub();
    expect(router.push('muse_1', {})).toBe(false);
  });

  it('events:channel_open opens its channel as persistent', () => {
    const actor = readFileSync(ACTOR, 'utf8');
    const open = actor.slice(actor.indexOf("case 'events:channel_open'"));
    const call = open.slice(0, open.indexOf("case 'events:channel_close'"));
    expect(call).toContain('{ persistent: true }');
  });
});
