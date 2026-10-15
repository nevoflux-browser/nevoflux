/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * What a generated canvas may do through NevofluxSDK (M3-0 design §4).
 *
 * The canvas iframe is isolated (a separate process, no access to the parent
 * page or chrome APIs), so canvas.js `_handleBridgeCall` is the only way its
 * code reaches anything privileged. Every rule lives here, DOM-free, so it is
 * unit-tested directly (tests/unit/canvas-bridge-policy.test.mjs) and
 * canvas.js imports this file rather than carrying a copy.
 */

const ALLOWED_METHODS = new Set([
  'callTool',
  'agent.chat',
  'agent.cancel',
  'agent.sendCommand',
  'sidebar.open',
  'sidebar.send',
  'sidebar.notify',
  'storage.get',
  'storage.set',
  'storage.delete',
  'storage.query',
  'system.getInfo',
  'canvas.share',
  'canvas.import',
  'canvas.share.extend',
  'canvas.share.delete',
  'canvas.share.list',
  'canvas.tool.invoke',
  'canvas.tool.list',
  'events.subscribe',
  'events.unsubscribe',
  'events.publish',
  'events.history',
  'events.recover',
]);

/** Whether the SDK method is one a canvas may call at all. */
export function isAllowedMethod(method) {
  return typeof method === 'string' && ALLOWED_METHODS.has(method);
}

// Kept in step with the daemon's canvas gate (canvas_gate.rs); both enforce it.
const FORBIDDEN_COMMAND_PREFIXES = [
  'config.',
  'remote.',
  'account.',
  'content_store.',
  'soul.',
  'local.',
  'kb.wizard.',
];
const FORBIDDEN_COMMANDS = new Set([
  'mcp.add',
  'mcp.update',
  'mcp.delete',
  'mcp.connect',
  'mcp.disconnect',
  'pack.install',
  'pack.update',
  'pack.uninstall',
  'jev.set',
  'models.download',
  'session.delete',
  'session.clone',
  'file.pick',
]);

/** Whether no canvas may send this daemon command, declared or not. */
export function isForbiddenCommand(command) {
  if (typeof command !== 'string') return true;
  return FORBIDDEN_COMMANDS.has(command) || FORBIDDEN_COMMAND_PREFIXES.some((p) => command.startsWith(p));
}

const RESERVED_STORAGE_PREFIXES = ['config:', 'canvas:', 'session:', 'app:'];

function requireArtifactId(artifactId) {
  if (typeof artifactId !== 'string' || !artifactId) {
    throw new Error('this canvas has no artifact id');
  }
  if (artifactId.includes(':')) {
    throw new Error('an artifact id cannot contain ":"');
  }
}

/** The ContentStore key a canvas's storage key lives under. */
export function appKey(artifactId, key) {
  requireArtifactId(artifactId);
  if (typeof key !== 'string' || !key) {
    throw new Error('a storage key must be a non-empty string');
  }
  return `app:${artifactId}:${key}`;
}

/** The ContentStore prefix a canvas's storage query runs under. */
export function appPrefix(artifactId, prefix = '') {
  requireArtifactId(artifactId);
  return `app:${artifactId}:${typeof prefix === 'string' ? prefix : ''}`;
}

/**
 * Whether a flat (pre-namespace) key may be read once to migrate it.
 * Keys under a reserved prefix belong to the browser or to another canvas.
 */
export function legacyKeyReadable(key) {
  return typeof key === 'string' && key.length > 0 && !RESERVED_STORAGE_PREFIXES.some((p) => key.startsWith(p));
}

/** Value stored under a deleted key so the legacy flat key is not read again. */
export const TOMBSTONE = Object.freeze({ __nevofluxDeleted: true });

/** Whether a stored value is the deletion marker. */
export function isTombstone(v) {
  return !!v && typeof v === 'object' && v.__nevofluxDeleted === true;
}

/**
 * What storage.get does with the value found under the namespaced key:
 * 'value' (return it), 'deleted' (return null, never consult the legacy key)
 * or 'legacy' (absent: try the flat key once and migrate it).
 */
export function storageGetPlan(nsValue) {
  if (nsValue === undefined || nsValue === null) return 'legacy';
  return isTombstone(nsValue) ? 'deleted' : 'value';
}

/**
 * A canvas's own query rows with the namespace stripped and deleted keys
 * dropped. Query never looks at flat legacy keys (spec section 4).
 */
export function ownQueryResults(artifactId, rows) {
  const base = appPrefix(artifactId, '');
  const out = [];
  for (const r of Array.isArray(rows) ? rows : []) {
    if (r && typeof r.key === 'string' && r.key.startsWith(base) && !isTombstone(r.value)) {
      out.push({ key: r.key.slice(base.length), value: r.value });
    }
  }
  return out;
}

const UUID_RE = /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/;

/** A session id bound to one canvas. */
export function mintSessionId(artifactId, uuid) {
  requireArtifactId(artifactId);
  return `cs_${artifactId}_${uuid}`;
}

/** Whether `sessionId` was minted for this canvas (prefix, then exactly a UUID). */
export function isOwnSession(artifactId, sessionId) {
  if (typeof artifactId !== 'string' || !artifactId || typeof sessionId !== 'string') return false;
  const prefix = `cs_${artifactId}_`;
  return sessionId.startsWith(prefix) && UUID_RE.test(sessionId.slice(prefix.length));
}

/**
 * The agent.chat request a canvas may make.
 * Images travel as attachments. Local paths do not: a path attachment puts
 * the file into the model's context without any tool gate, so the paths are
 * named in the message and the agent reads them with its (tier-gated) tools.
 */
export function chatRequest(args) {
  const raw = args && args.message !== undefined ? args.message : args;
  const message = typeof raw === 'string' ? raw : JSON.stringify(raw);
  const images = [];
  const paths = [];
  for (const a of Array.isArray(args?.attachments) ? args.attachments : []) {
    if (a && typeof a.data === 'string' && typeof a.mime_type === 'string') {
      images.push({ name: typeof a.name === 'string' ? a.name : 'image', mime_type: a.mime_type, data: a.data });
    } else if (a && typeof a.path === 'string' && a.path) {
      paths.push(a.is_directory ? `${a.path} (directory)` : a.path);
    }
  }
  const note = paths.length
    ? '\n\n[The canvas referred to these local paths. They were not attached; ' +
      'read them with your file tools if the task needs them.]\n' +
      paths.map((p) => `- ${p}`).join('\n')
    : '';
  const out = { message: message + note };
  if (images.length) out.attachments = images;
  return out;
}

const SIDEBAR_TEXT_MAX = 4000;

/** The user message sidebar.send injects, labelled with its canvas. */
export function sidebarText(args, artifactId) {
  const raw = args && args.message !== undefined ? args.message : args;
  if (typeof raw !== 'string') throw new Error('sidebar.send takes a string');
  if (raw.length > SIDEBAR_TEXT_MAX) {
    throw new Error(`sidebar.send text is limited to ${SIDEBAR_TEXT_MAX} characters`);
  }
  return `[Canvas ${artifactId}] ${raw}`;
}

/**
 * The fixed-form notification sidebar.notify shows. With a label the title
 * names the canvas ("Canvas: <label>"), so the user can tell who is asking.
 */
export function notifyContent(type, data, label) {
  const t = (typeof type === 'string' && type ? type : 'Canvas').slice(0, 80);
  const l = typeof label === 'string' && label ? label.slice(0, 60) : '';
  let title = t;
  if (l) title = t === 'Canvas' ? `Canvas: ${l}` : `Canvas: ${l} — ${t}`;
  let message = '';
  if (typeof data === 'string') message = data;
  else if (data && typeof data === 'object') {
    if (typeof data.message === 'string') message = data.message;
    else if (typeof data.body === 'string') message = data.body;
  }
  return { title, message: message.slice(0, 300) };
}

// Topics the browser itself consumes; a canvas publishing there could fake a
// notification, swap the avatar or drive a tab.
const RESERVED_TOPIC_PREFIXES = ['ui:notification', 'ui:soul', 'ui:render', 'ui:tab'];

/** Whether a canvas may not publish to this topic. */
export function isReservedTopic(topic) {
  if (typeof topic !== 'string' || !topic) return true;
  return RESERVED_TOPIC_PREFIXES.some((p) => topic === p || topic.startsWith(p + ':'));
}

/** Share requests always name the calling canvas. */
export function shareArgs(method, args, artifactId) {
  const a = args && typeof args === 'object' ? args : {};
  return method === 'canvas.share' ? { ...a, artifact_id: artifactId } : a;
}

/**
 * The shares in a canvas.share.list answer that belong to this canvas.
 * Accepts the bare array or `{ shares: [...] }` and returns the same shape;
 * anything else is treated as no shares.
 */
export function ownShares(listResult, artifactId) {
  const mine = (list) =>
    list.filter((s) => s && typeof s === 'object' && s.artifact_id === artifactId);
  if (Array.isArray(listResult)) return mine(listResult);
  if (listResult && typeof listResult === 'object' && Array.isArray(listResult.shares)) {
    return { ...listResult, shares: mine(listResult.shares) };
  }
  return [];
}

/** Whether `shareId` names one of this canvas's own shares in a list answer. */
export function isOwnShare(listResult, artifactId, shareId) {
  if (typeof shareId !== 'string' || !shareId) return false;
  const own = ownShares(listResult, artifactId);
  const list = Array.isArray(own) ? own : own.shares;
  return list.some((s) => s.share_id === shareId);
}

/**
 * Unwrap a bridge:request answer. The parent actor wraps background's result
 * ({success, data|error}) in its own {success, data}; either layer can fail.
 */
export function unwrapBridgeResult(res, label) {
  if (!res || res.success === false) {
    throw new Error(res?.error?.message || `${label} failed`);
  }
  const inner = res.data;
  if (inner && typeof inner === 'object' && 'success' in inner) {
    if (inner.success === false) throw new Error(inner.error?.message || `${label} failed`);
    return inner.data;
  }
  return inner;
}
