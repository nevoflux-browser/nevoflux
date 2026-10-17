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
 *    after the task, so it never decides hiding/holding by itself;
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
 * Remember one `session_origin` payload
 * (`{session_id, origin: {kind: "muse", pairing}}`). Returns whether it was
 * usable. A session seen again (a continued task) becomes the newest.
 */
export function recordSessionOrigin(sessions, payload) {
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
 * without an `agents` array is ignored. Returns whether the set changed.
 */
export function applyStatusSnapshot(live, payload) {
  if (!Array.isArray(payload?.agents)) {
    return false;
  }
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
 * Whether `sessionId` is delegated for hiding/holding questions, background
 * Canvas and badges: a task is running in it. Takes the live set, not the
 * origin map.
 */
export function isDelegated(live, sessionId) {
  return hasLiveTask(live, sessionId);
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
