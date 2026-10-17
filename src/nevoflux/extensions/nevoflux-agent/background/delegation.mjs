/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Delegated tasks as the background script sees them (M3 §6).
 *
 * A paired agent (Muse) can hand NevoFlux a whole task. The daemon runs it as
 * a session of this extension's own chat connection and announces it first
 * with a `session_origin` frame (contract R19). What the extension does
 * differently for those sessions is decided here, DOM-free so Node can test it:
 *  - which sessions are delegated: those with a task RUNNING now, taken from
 *    the system:agent:status snapshot (ruling B-F2). A session_origin only
 *    records where a session came from (history badges); it is re-sent at the
 *    end of every delegated turn and a person may keep typing into the session
 *    after the task, so it never decides hiding/holding by itself — except
 *    provisionally (ruling B-T2-1): the snapshot travels on the event bus and
 *    session_origin on the chat connection, so a delegated turn's first
 *    question or Canvas can beat the snapshot that lists its task. A session
 *    whose session_origin arrived after the latest snapshot counts as
 *    delegated until the next snapshot, which is authoritative, or for at
 *    most 30 s (ruling B-T2-2). A question there is held only if it also
 *    carries the daemon's delegated stamp (contract R32);
 *  - Canvas tabs per session, so one conversation never closes another's;
 *  - which canvas-chat session a daemon frame belongs to;
 *  - how long a delegated question may wait in this script.
 */

/** How many delegated sessions are remembered (the oldest is forgotten first). */
export const DELEGATED_SESSION_CAP = 200;

/**
 * How long a delegated question stays open in this script. The daemon owns
 * the real deadline (1800 s, contract §11) and settles it with
 * browser_tool_resolved; this only keeps a lost answer from leaking the
 * entry. It must outlast the daemon's, or this timer would answer "timed out"
 * first and the daemon would read that as a denial.
 */
export const DELEGATED_ASK_HOLD_MS = 31 * 60 * 1000;

/**
 * The `params.timeout_ms` the daemon stamps on a delegated task's own
 * question (contract R32: exactly this value). A local chat's question
 * carries 60 000; permission-style asks carry 86 400 000 whoever's they are.
 */
export const DELEGATED_ASK_TIMEOUT_MS = 1_800_000;

/**
 * How long a session stays provisionally delegated after the session_origin
 * that made it so, when no snapshot settles it first (ruling B-T2-2; longer
 * than the daemon's 5 s turn-end grace). Bounds what a missed or rejected
 * snapshot can do: background Canvas, the badge, direct tool routing.
 */
export const PROVISIONAL_DELEGATION_MS = 30_000;

/**
 * Remember one `session_origin` payload
 * (`{session_id, origin: {kind: "muse", pairing}}`). Returns whether it was
 * usable. A session seen again (a continued task) becomes the newest. With
 * `pending` (a Map of session id -> expiry ms), a usable origin also makes
 * the session provisionally delegated until the next status snapshot or
 * `now + PROVISIONAL_DELEGATION_MS`, whichever comes first (rulings B-T2-1,
 * B-T2-2); a turn-end re-send (contract v1.3 R19) restarts that, harmlessly.
 * Expired entries are dropped here, so the map stays small.
 */
export function recordSessionOrigin(sessions, payload, pending = null, now = Date.now()) {
  const sessionId = payload?.session_id;
  const origin = payload?.origin;
  if (typeof sessionId !== 'string' || !sessionId) {
    return false;
  }
  if (origin?.kind !== 'muse' || typeof origin.pairing !== 'string') {
    return false;
  }
  sessions.delete(sessionId);
  sessions.set(sessionId, { pairing: origin.pairing });
  while (sessions.size > DELEGATED_SESSION_CAP) {
    sessions.delete(sessions.keys().next().value);
  }
  if (pending) {
    for (const [sid, expiresAt] of pending) {
      if (expiresAt <= now) {
        pending.delete(sid);
      }
    }
    pending.set(sessionId, now + PROVISIONAL_DELEGATION_MS);
  }
  return true;
}

/** The recorded origin `{pairing}` of a session, or null. Not a liveness test. */
export function originOf(sessions, sessionId) {
  if (typeof sessionId !== 'string' || !sessionId) {
    return null;
  }
  return sessions.get(sessionId) ?? null;
}

/**
 * Replace `live` (a Set of session ids) with the sessions that have a task in
 * this system:agent:status snapshot (`agents[].task.session_id`, offline
 * entries included). Every snapshot is FULL: replace, never merge. A payload
 * without an `agents` array is ignored. Returns whether the live set changed.
 * With `pending`, every well-formed snapshot also ends all provisional
 * delegations (ruling B-T2-1): from now on the snapshot decides.
 */
export function applyStatusSnapshot(live, payload, pending = null) {
  if (!Array.isArray(payload?.agents)) {
    return false;
  }
  pending?.clear();
  const next = new Set();
  for (const agent of payload.agents) {
    const sid = agent?.task?.session_id;
    if (typeof sid === 'string' && sid) {
      next.add(sid);
    }
  }
  let changed = next.size !== live.size;
  for (const sid of next) {
    if (!live.has(sid)) {
      changed = true;
    }
  }
  if (changed) {
    live.clear();
    for (const sid of next) {
      live.add(sid);
    }
  }
  return changed;
}

/** Whether a task is running in `sessionId` now. */
export function hasLiveTask(live, sessionId) {
  return typeof sessionId === 'string' && sessionId.length > 0 && live.has(sessionId);
}

/**
 * Whether `sessionId` is provisionally delegated at `now`: its session_origin
 * arrived after the latest snapshot less than PROVISIONAL_DELEGATION_MS ago.
 * An expired entry is dropped.
 */
function isProvisional(pending, sessionId, now) {
  if (!pending || typeof sessionId !== 'string' || !sessionId || !pending.has(sessionId)) {
    return false;
  }
  if (pending.get(sessionId) > now) {
    return true;
  }
  pending.delete(sessionId);
  return false;
}

/**
 * Whether `sessionId` is delegated for background Canvas, badges and direct
 * tool routing: a task is running in it, or (with `pending`) it is
 * provisionally delegated at `now` (rulings B-T2-1, B-T2-2). Takes the live
 * set, not the origin map. Questions use askIsDelegated instead.
 */
export function isDelegated(live, sessionId, pending = null, now = Date.now()) {
  return hasLiveTask(live, sessionId) || isProvisional(pending, sessionId, now);
}

/**
 * Whether an AskUser in `sessionId` is a delegated task's question, to be
 * held and not shown (contract R21; rulings B-T2-2, B-T2-3): the session has
 * a live task (then permission asks are the task's too), or it is
 * provisionally delegated AND `timeout_ms` is exactly the daemon's delegated
 * stamp DELEGATED_ASK_TIMEOUT_MS (R32). Exact, not "at least": a person's
 * own permission ask carries 86 400 000, and one arriving in the provisional
 * window (say after a turn-end session_origin) must be shown, not hidden for
 * 24 h. The decision is taken once, so it must not guess.
 */
export function askIsDelegated(live, pending, sessionId, timeoutMs, now = Date.now()) {
  if (hasLiveTask(live, sessionId)) {
    return true;
  }
  return timeoutMs === DELEGATED_ASK_TIMEOUT_MS && isProvisional(pending, sessionId, now);
}

/**
 * Runs async jobs one after another per session, in call order; different
 * sessions do not wait for each other. Two Canvas opens for one session at
 * once would otherwise both see "no tab yet" and leave one tab orphaned.
 * A job's result or error is passed through; a failure does not block the
 * next job. A session with nothing queued is forgotten.
 */
export class SessionQueue {
  constructor() {
    this.tails = new Map();
  }

  run(sessionId, job) {
    const previous = this.tails.get(sessionId) ?? Promise.resolve();
    const result = previous.then(job);
    const tail = result.then(
      () => {},
      () => {}
    );
    this.tails.set(sessionId, tail);
    tail.then(() => {
      if (this.tails.get(sessionId) === tail) {
        this.tails.delete(sessionId);
      }
    });
    return result;
  }
}

/** How many delivered event ids StatusSnapshotGate remembers. */
const STATUS_EVENT_ID_CAP = 64;

/**
 * Which system:agent:status deliveries are new snapshots. A reconnect's
 * sticky replay redelivers snapshots already applied, oldest first; applying
 * one again would end provisional delegations that started after it, and an
 * older one would bring back tasks that have ended. So a delivery is admitted
 * only when it is well formed, its `event_id` is not one already admitted,
 * and its `timestamp_ms` is not older than the newest admitted (the sidebar's
 * rule). A delivery without an id or a time is admitted on what it has.
 * `timestamp_ms` is the daemon's wall clock: if it steps back, later
 * snapshots are dropped until it passes the newest admitted again (the
 * provisional 30 s bound keeps that from holding sessions meanwhile).
 */
export class StatusSnapshotGate {
  constructor() {
    this.newestMs = null;
    this.seen = [];
  }

  /** Whether `event` (`{event_id, timestamp_ms, payload}`) is a new snapshot. Records it. */
  admit(event) {
    if (!Array.isArray(event?.payload?.agents)) {
      return false;
    }
    const id = typeof event.event_id === 'string' && event.event_id ? event.event_id : null;
    if (id !== null && this.seen.includes(id)) {
      return false;
    }
    const ms = Number.isFinite(event.timestamp_ms) ? event.timestamp_ms : null;
    if (ms !== null && this.newestMs !== null && ms < this.newestMs) {
      return false;
    }
    if (ms !== null) {
      this.newestMs = ms;
    }
    if (id !== null) {
      this.seen.push(id);
      if (this.seen.length > STATUS_EVENT_ID_CAP) {
        this.seen.shift();
      }
    }
    return true;
  }
}

/** How long a question may wait here before this script answers "timed out". */
export function askHoldMs(delegated, requestedMs) {
  if (delegated) {
    return DELEGATED_ASK_HOLD_MS;
  }
  return Number.isFinite(requestedMs) && requestedMs > 0 ? requestedMs : 60000;
}

/** Canvas tabs by chat session (M3 §6.1): one tab per session at most. */
export class CanvasTabs {
  constructor() {
    this.bySession = new Map();
  }

  /** The session's Canvas tab, or null. */
  tabFor(sessionId) {
    return this.bySession.has(sessionId) ? this.bySession.get(sessionId) : null;
  }

  set(sessionId, tabId) {
    this.bySession.set(sessionId, tabId);
  }

  /** A tab closed: the session it belonged to has no Canvas tab now. */
  forgetTab(tabId) {
    for (const [sessionId, owned] of this.bySession) {
      if (owned === tabId) {
        this.bySession.delete(sessionId);
      }
    }
  }
}

/**
 * How to open a Canvas tab for `sessionId`, or null when the frame names no
 * session: then no tab is opened and no other session's tab is touched
 * (contract §6). Only the same session's previous tab is replaced. A
 * delegated session's tab opens in the background (R20) with the agent badge.
 */
export function canvasOpenPlan(sessionId, tabs, delegated) {
  if (typeof sessionId !== 'string' || !sessionId) {
    return null;
  }
  return {
    replaceTabId: tabs.tabFor(sessionId),
    inBackground: delegated === true,
    markAgent: delegated === true,
  };
}

/**
 * The canvas-chat session (NevofluxSDK.agent.chat) a daemon frame belongs to,
 * or null. A frame that names a session goes only to that session; only a
 * frame with no session id falls back to the active canvas chat, so another
 * conversation's output (a delegated task's, the sidebar's own) is never
 * pushed into a canvas or allowed to end its chat.
 */
export function canvasChatSessionFor(payload, canvasSessions, activeSessionId) {
  const sid = payload?.session_id;
  if (typeof sid === 'string' && sid) {
    return canvasSessions.has(sid) ? sid : null;
  }
  return activeSessionId && canvasSessions.has(activeSessionId) ? activeSessionId : null;
}
