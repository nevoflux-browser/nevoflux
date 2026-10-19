/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { readFileSync } from 'node:fs';
import { describe, it, expect } from './test-runner.mjs';
import { LatestPersister } from '../../extensions/nevoflux-agent/background/latest-persist.mjs';

// A daemon whose acknowledgements the test releases one at a time.
function fakeDaemon() {
  const writes = [];
  const write = (operation, key, value) =>
    new Promise((resolve, reject) => {
      writes.push({ operation, key, value, resolve, reject });
    });
  return { write, writes };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('LatestPersister', () => {
  it('a value set while a write is in flight is written after it (the Canvas truncation)', async () => {
    const daemon = fakeDaemon();
    const p = new LatestPersister(daemon.write);
    const first = p.persist('canvas:a', 'set', { code: '<div>' });
    const last = p.persist('canvas:a', 'set', { code: '<div></div>' });
    expect(first).toBe(last);
    expect(daemon.writes.length).toBe(1);
    daemon.writes[0].resolve();
    await tick();
    expect(daemon.writes.length).toBe(2);
    expect(daemon.writes[1].value.code).toBe('<div></div>');
    daemon.writes[1].resolve();
    await last;
    expect(p.busy('canvas:a')).toBe(false);
  });

  it('only the newest of several queued values is written', async () => {
    const daemon = fakeDaemon();
    const p = new LatestPersister(daemon.write);
    const done = p.persist('k', 'set', 1);
    p.persist('k', 'set', 2);
    p.persist('k', 'set', 3);
    daemon.writes[0].resolve();
    await tick();
    expect(daemon.writes.map((w) => w.value)).toEqual([1, 3]);
    daemon.writes[1].resolve();
    await done;
  });

  it('a value equal to the one in flight is not written twice', async () => {
    const daemon = fakeDaemon();
    const p = new LatestPersister(daemon.write);
    const done = p.persist('k', 'set', { code: 'x' });
    p.persist('k', 'set', { code: 'x' });
    daemon.writes[0].resolve();
    await done;
    expect(daemon.writes.length).toBe(1);
  });

  it('a newer value queued and then reverted to the one in flight is not written', async () => {
    const daemon = fakeDaemon();
    const p = new LatestPersister(daemon.write);
    const done = p.persist('k', 'set', 'a');
    p.persist('k', 'set', 'b');
    p.persist('k', 'set', 'a');
    daemon.writes[0].resolve();
    await done;
    expect(daemon.writes.length).toBe(1);
  });

  it('keys do not wait for each other', () => {
    const daemon = fakeDaemon();
    const p = new LatestPersister(daemon.write);
    p.persist('a', 'set', 1);
    p.persist('b', 'set', 1);
    expect(daemon.writes.map((w) => w.key)).toEqual(['a', 'b']);
  });

  it('a failed write still writes the queued value; the chain fails only if the last write fails', async () => {
    const daemon = fakeDaemon();
    const p = new LatestPersister(daemon.write);
    const done = p.persist('k', 'set', 1);
    p.persist('k', 'set', 2);
    daemon.writes[0].reject(new Error('timed out'));
    await tick();
    expect(daemon.writes.length).toBe(2);
    daemon.writes[1].resolve();
    await done;

    const lone = p.persist('k', 'set', 3);
    daemon.writes[2].reject(new Error('timed out'));
    let error = null;
    try {
      await lone;
    } catch (e) {
      error = e;
    }
    expect(error?.message).toBe('timed out');
    expect(p.busy('k')).toBe(false);
  });

  it('a delete queued behind a set is written', async () => {
    const daemon = fakeDaemon();
    const p = new LatestPersister(daemon.write);
    const done = p.persist('k', 'set', 1);
    p.persist('k', 'delete', undefined);
    daemon.writes[0].resolve();
    await tick();
    expect(daemon.writes[1].operation).toBe('delete');
    daemon.writes[1].resolve();
    await done;
  });

  it('background.js persists Canvas keys through it, never skipping an in-flight key', () => {
    const bg = readFileSync(
      new URL('../../extensions/nevoflux-agent/background/background.js', import.meta.url),
      'utf8'
    );
    expect(bg.includes("import { LatestPersister } from './latest-persist.mjs';")).toBe(true);
    expect(bg.includes('artifactPersistAcks')).toBe(false);
    expect(bg.includes('Skipping listener persist')).toBe(false);
    expect((bg.match(/artifactPersister\.persist\(key, /g) || []).length).toBe(2);
  });
});
