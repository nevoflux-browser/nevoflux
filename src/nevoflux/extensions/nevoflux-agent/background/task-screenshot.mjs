/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * The daemon's own screenshot of a delegated task's result (M3 §5,
 * contract §7), as rules Node can test.
 *
 * The daemon sends it as a browser_tool_request with `origin: "system"`. It
 * deliberately skips the agent tab rules (an agent may not touch a
 * nevoflux:// tab; the daemon looking at the Canvas a task made must), so
 * nothing else may ever carry that origin: background.js strips it from
 * Canvas bridge requests and executeBrowserTool refuses it from any caller.
 */

export const SYSTEM_ORIGIN = 'system';

/** Contract §11 limits. A request may lower them, never raise them. */
export const TASK_SCREENSHOT_LIMITS = Object.freeze({
  quality: 60,
  retryQuality: 40,
  maxWidth: 1280,
  maxHeight: 2400,
  maxBytes: 307200,
});

const SCREENSHOT_ERRORS = new Set(['tab_discarded', 'tab_closed', 'too_large', 'capture_failed']);

/** Whether a browser_tool_request is the daemon's own (origin "system"). */
export function isSystemRequest(request) {
  return request?.origin === SYSTEM_ORIGIN;
}

/**
 * Why a system request may not run, or null: only the task screenshot
 * exists. The answer is one of contract §7's four messages.
 */
export function systemRequestRefusal(request) {
  if (request?.action === 'screenshot' && request?.params?.purpose === 'task_result') {
    return null;
  }
  return { code: -1, message: 'capture_failed', recoverable: false };
}

/** The refusal executeBrowserTool gives a system request from any caller. */
export function systemOriginRefusal(action) {
  return {
    code: -1,
    message: `a system request cannot run '${action}' as a browser tool`,
    recoverable: false,
  };
}

/** A copy of a request without `origin`: what a Canvas bridge request becomes. */
export function withoutOrigin(payload) {
  if (!payload || typeof payload !== 'object') {
    return payload;
  }
  const { origin: _origin, ...rest } = payload;
  return rest;
}

function capped(value, limit) {
  return Number.isInteger(value) && value > 0 ? Math.min(value, limit) : limit;
}

/** The capture options for a request's params (contract §7 names). */
export function taskScreenshotOptions(params) {
  const p = params && typeof params === 'object' ? params : {};
  const L = TASK_SCREENSHOT_LIMITS;
  return {
    quality: capped(p.quality, L.quality),
    retryQuality: capped(p.retry_quality, L.retryQuality),
    maxWidth: capped(p.max_width, L.maxWidth),
    maxHeight: capped(p.max_height, L.maxHeight),
    maxBytes: capped(p.max_bytes, L.maxBytes),
  };
}

/** Decoded size of a base64 string. */
export function base64Bytes(b64) {
  if (typeof b64 !== 'string' || b64.length === 0) {
    return 0;
  }
  const pad = b64.endsWith('==') ? 2 : b64.endsWith('=') ? 1 : 0;
  return Math.floor((b64.length * 3) / 4) - pad;
}

/** A capture failure as one of the four contract messages. */
export function screenshotError(error) {
  const message = error?.message;
  return SCREENSHOT_ERRORS.has(message) ? message : 'capture_failed';
}

function failure(message) {
  return { success: false, error: { code: -1, message, recoverable: false } };
}

/**
 * Capture at the first quality, and once more at the retry quality if the
 * JPEG is over the byte budget; still over is too_large. `capture(quality)`
 * resolves to the parent API's result. The answer is the contract §7
 * response body.
 */
export async function captureWithinBudget(capture, opts) {
  for (const quality of [opts.quality, opts.retryQuality]) {
    let shot;
    try {
      shot = await capture(quality);
    } catch (e) {
      return failure('capture_failed');
    }
    if (!shot || shot.success !== true) {
      return failure(screenshotError(shot?.error));
    }
    const data = typeof shot.data === 'string' ? shot.data.replace(/^data:[^,]*,/, '') : '';
    const bytes = base64Bytes(data);
    if (bytes === 0) {
      return failure('capture_failed');
    }
    if (bytes <= opts.maxBytes) {
      return {
        success: true,
        result: {
          mime_type: 'image/jpeg',
          data,
          width: shot.width,
          height: shot.height,
          bytes,
          tab_kind: shot.tabKind === 'canvas' ? 'canvas' : 'web',
        },
      };
    }
  }
  return failure('too_large');
}

/** Whether a tab shows nevoflux://canvas/<artifactId> (a1 is not a10). */
function showsArtifact(tab, artifactId) {
  const url = `nevoflux://canvas/${artifactId}`;
  return typeof tab?.url === 'string' && (tab.url === url || tab.url.startsWith(`${url}?`) || tab.url.startsWith(`${url}#`));
}

/** The id of the tab showing nevoflux://canvas/<artifactId>, or null. */
export function canvasTabForArtifact(tabs, artifactId) {
  if (typeof artifactId !== 'string' || !artifactId) {
    return null;
  }
  const tab = (Array.isArray(tabs) ? tabs : []).find((t) => showsArtifact(t, artifactId));
  return tab && Number.isInteger(tab.id) ? tab.id : null;
}

/**
 * The tab a task screenshot captures, or null (answered tab_closed).
 *
 * A web target (`tab_id` > 0) is that tab. A Canvas target (`tab_id` null +
 * `params.artifact_id`) is the tab showing that artifact; the session's Canvas
 * tab only counts while it shows it, since a later Canvas of the same session
 * replaces it and a picture of B must never stand in for A. A bad `tab_id`
 * is no tab, never a fall-through to a Canvas.
 */
export function taskScreenshotTabId(request, sessionTabId, tabs) {
  const tabId = request?.tab_id;
  if (Number.isInteger(tabId) && tabId > 0) {
    return tabId;
  }
  if (tabId !== null && tabId !== undefined) {
    return null;
  }
  const artifactId = request?.params?.artifact_id;
  if (typeof artifactId !== 'string' || !artifactId) {
    return null;
  }
  const list = Array.isArray(tabs) ? tabs : [];
  const shown = canvasTabForArtifact(list, artifactId);
  if (shown !== null) {
    return shown;
  }
  const sessionTab = list.find((t) => t?.id === sessionTabId);
  return sessionTab && Number.isInteger(sessionTabId) && showsArtifact(sessionTab, artifactId) ? sessionTabId : null;
}

/** How long a Canvas target may take to open and load (the request has 30 s). */
export const CANVAS_TAB_WAIT_MS = 10000;
const CANVAS_TAB_POLL_MS = 200;
/** After waiting for a Canvas tab to load: time for its page to draw the artifact. */
export const CANVAS_SETTLE_MS = 750;

const sleepMs = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * The tab a task screenshot captures, waiting for a Canvas target to show up.
 *
 * The daemon asks the moment the turn ends, while this script may still be
 * opening the tab for the artifact the task just made: artifact_complete and
 * the request arrive together, and the tab opens only after the artifact is
 * stored. `lookup()` resolves to `{ tabId, loaded }` or null. A Canvas target
 * is polled until its tab exists and has loaded (then given `settleMs` to
 * draw), or until `waitMs` passes: a tab that exists by then is used as it
 * is, none is null (tab_closed). A web target is looked up once.
 */
export async function awaitTaskTab(
  request,
  lookup,
  { waitMs = CANVAS_TAB_WAIT_MS, pollMs = CANVAS_TAB_POLL_MS, settleMs = CANVAS_SETTLE_MS, sleep = sleepMs, now = Date.now } = {}
) {
  let found = await lookup();
  const artifactId = request?.params?.artifact_id;
  const canvasTarget =
    (request?.tab_id === null || request?.tab_id === undefined) && typeof artifactId === 'string' && artifactId !== '';
  if (!canvasTarget || found?.loaded) {
    return found ? found.tabId : null;
  }
  const deadline = now() + waitMs;
  while (now() < deadline) {
    await sleep(pollMs);
    found = await lookup();
    if (found?.loaded) {
      await sleep(settleMs);
      return found.tabId;
    }
  }
  return found ? found.tabId : null;
}
