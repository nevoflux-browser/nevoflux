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

/** Why a system request may not run, or null: only the task screenshot exists. */
export function systemRequestRefusal(request) {
  if (request?.action === 'screenshot' && request?.params?.purpose === 'task_result') {
    return null;
  }
  return {
    code: -1,
    message: `system request '${request?.action}' is not supported; only the task_result screenshot is`,
    recoverable: false,
  };
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

/** The id of the tab showing nevoflux://canvas/<artifactId>, or null. */
export function canvasTabForArtifact(tabs, artifactId) {
  if (typeof artifactId !== 'string' || !artifactId) {
    return null;
  }
  const url = `nevoflux://canvas/${artifactId}`;
  const tab = (Array.isArray(tabs) ? tabs : []).find(
    (t) => typeof t?.url === 'string' && (t.url === url || t.url.startsWith(`${url}?`) || t.url.startsWith(`${url}#`))
  );
  return tab && Number.isInteger(tab.id) ? tab.id : null;
}
