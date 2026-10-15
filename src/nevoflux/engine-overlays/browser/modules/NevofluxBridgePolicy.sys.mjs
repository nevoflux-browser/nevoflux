/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Which actor messages a document may send to NevofluxParent (M3-0 §5).
 *
 * Defence in depth behind canvas-bridge-policy.mjs: the Nevoflux actor is
 * registered on every frame, so this keeps a frame that is not a nevoflux://
 * page (the canvas iframe, a web page) from using the bridge at all, and keeps
 * each nevoflux:// page to the high-risk types it actually needs. Pure and
 * DOM-free; tested by tests/unit/nevoflux-bridge-policy.test.mjs.
 */

const ALLOW = Object.freeze({ allow: true });
const deny = (reason) => ({ allow: false, reason });

// Sent by the child on ordinary web pages (dialogs, Record & Replay).
const OPEN_TO_ANY_DOCUMENT = new Set([
  'dialogAccept',
  'dialogDismiss',
  'recording:event',
  'recording:isActive',
]);

const COMMAND_LANES = {
  settings: () => true,
  brain: (c) => c.startsWith('brain.'),
  'pack-install': (c) => c.startsWith('pack.'),
};

const SETTINGS_ONLY_TYPES = new Set([
  'getCache',
  'canvas.tool.save',
  'canvas.tool.delete',
  'canvas.tool.get_raw',
  'canvas.persist.list',
  'canvas.persist.rename',
  'canvas.persist.delete',
]);

const CANVAS_ONLY_TYPES = new Set(['canvas.command', 'canvas:notify']);

const CANVAS_SYSTEM_COMMANDS = new Set(['content_store.load', 'artifact.get']);

const RENDER_MESSAGE_TYPES = new Set([
  'canvas_video_frame_chunk',
  'canvas_video_render_done',
  'canvas_video_render_failed',
]);

/** The nevoflux:// page a document is, e.g. "canvas", or null. */
export function pageOf(uriSpec) {
  if (typeof uriSpec !== 'string') return null;
  let m = /^nevoflux:\/\/([a-z0-9-]+)(?:[/?#]|$)/i.exec(uriSpec);
  if (m) return m[1].toLowerCase();
  m = /^chrome:\/\/nevoflux\/content\/(?:nevoflux\/)?pages\/([a-z0-9-]+)\.html(?:[?#]|$)/i.exec(uriSpec);
  return m ? m[1].toLowerCase() : null;
}

function checkStoreWrite(page, key) {
  if (typeof key !== 'string' || !key) return deny('a ContentStore key must be a non-empty string');
  if (page === 'settings') return ALLOW;
  if (key.startsWith('config:')) return deny('only the settings page may write config:* keys');
  if (page === 'canvas' && !key.startsWith('canvas:') && !key.startsWith('app:')) {
    return deny('the canvas page may only write canvas:* and app:* keys');
  }
  return ALLOW;
}

function checkSendToAgent(page, payload) {
  if (
    page === 'canvas' &&
    payload?.type === 'system_command' &&
    CANVAS_SYSTEM_COMMANDS.has(payload?.payload?.command)
  ) {
    return ALLOW;
  }
  if (page === 'render' && RENDER_MESSAGE_TYPES.has(payload?.type)) return ALLOW;
  return deny(`${page} may not send this message to the agent`);
}

function checkBridgeRequest(page, type, payload) {
  if (typeof type !== 'string' || !type) return deny('bridge request without a type');
  if (type === 'agent:command') {
    const lane = COMMAND_LANES[page];
    const command = payload?.command;
    if (!lane) return deny(`${page} may not send daemon commands`);
    if (typeof command !== 'string' || !lane(command)) return deny(`${page} may not send ${command}`);
    return ALLOW;
  }
  if (SETTINGS_ONLY_TYPES.has(type)) {
    return page === 'settings' ? ALLOW : deny(`${type} is only available to the settings page`);
  }
  if (CANVAS_ONLY_TYPES.has(type)) {
    return page === 'canvas' ? ALLOW : deny(`${type} is only available to the canvas page`);
  }
  if (type === 'sidebar_send') return deny('sidebar_send is not available to pages');
  if (type === 'send_to_agent') return checkSendToAgent(page, payload);
  return ALLOW;
}

/** Whether a document at `uriSpec` may send actor message `name` with `data`. */
export function checkActorMessage(uriSpec, name, data) {
  if (OPEN_TO_ANY_DOCUMENT.has(name)) return ALLOW;
  const page = pageOf(uriSpec);
  if (!page) return deny(`${name} is only available to nevoflux:// pages`);
  if (name === 'contentStore:set' || name === 'contentStore:delete') {
    return checkStoreWrite(page, data?.key);
  }
  if (name === 'bridge:request') return checkBridgeRequest(page, data?.type, data?.payload);
  return ALLOW;
}

// NevofluxParent reaches this module through a lazy getter named after it
// (ChromeUtils.defineESModuleGetters), which resolves this export by name.
export const NevofluxBridgePolicy = Object.freeze({ pageOf, checkActorMessage });
