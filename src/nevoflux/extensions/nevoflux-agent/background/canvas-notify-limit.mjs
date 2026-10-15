/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

export const NOTIFY_LIMIT = 3;
export const NOTIFY_WINDOW_MS = 30000;

/**
 * Rolling-window limiter for canvas notifications, one bucket per artifact
 * (a missing id shares the '' bucket). `history` is a Map of artifact id to
 * timestamps. Returns true and records the notification when allowed.
 */
export function allowCanvasNotify(history, artifactId, nowMs) {
  const id = typeof artifactId === 'string' ? artifactId : '';
  const recent = (history.get(id) || []).filter((t) => nowMs - t < NOTIFY_WINDOW_MS);
  if (recent.length >= NOTIFY_LIMIT) {
    history.set(id, recent);
    return false;
  }
  recent.push(nowMs);
  history.set(id, recent);
  return true;
}
