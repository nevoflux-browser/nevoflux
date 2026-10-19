/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * DOM-free logic backing the Muse section on nevoflux://settings: the
 * paired AI agents, each one's execution tier, and whether Muse may approve
 * sensitive actions for its tasks (M3 contract v1.2 R39). Kept separate from
 * settings.js so it can be unit-tested without a browser; settings.js
 * imports it dynamically from chrome://nevoflux/content/pages/.
 *
 * Inputs are the daemon's `remote.pairings` rows and the sticky
 * `system:agent:status` snapshot (the same payload the sidebar's
 * state/paired_agents.rs reads).
 */

/** The sticky EventBus topic the daemon publishes the agent snapshot on. */
export const AGENT_STATUS_TOPIC = 'system:agent:status';

/** The tiers, least to most. Exactly the daemon's `ExecutionTier` strings. */
export const TIERS = ['read-only', 'browser-auto', 'browser-auto-local-read', 'full-auto'];

/** What the tier select shows, as the General section's "Agent execution". */
export const TIER_LABELS = {
  'read-only': 'Read-only',
  'browser-auto': 'Browser auto',
  'browser-auto-local-read': 'Browser auto + reads',
  'full-auto': 'Full auto',
};

/** Shown when a pairing has no label (the sidebar's DEFAULT_LABEL). */
export const DEFAULT_LABEL = 'Muse';

/** The switch and its help line (contract v1.2 R39, exact copy). */
export const GUARDED_LABEL = 'Let Muse approve sensitive actions';
export const GUARDED_HELP =
  'Muse can approve scheduling, packs, model switching, screen sharing and prompt changes for its tasks. Off: only you can, here.';

export const TIER_SELECT_LABEL = 'Execution tier';

export const INTRO_TEXT =
  'AI agents you paired with /pair-agent. Each one has its own execution tier, ' +
  'separate from the General "Agent execution" setting. Pause, resume and ' +
  'unpair them from the sidebar.';

export const EMPTY_TEXT = 'No AI agent is paired. Type /pair-agent in the sidebar to pair one.';

/** How long a failed command's error line stays under its card. */
export const ERROR_LINE_MS = 5000;

/** Whether `t` is one of the four tier strings. */
export function isTier(t) {
  return typeof t === 'string' && TIERS.includes(t);
}

/** A pairing's display name: its label, trimmed, or "Muse". */
export function displayName(label) {
  const l = typeof label === 'string' ? label.trim() : '';
  return l || DEFAULT_LABEL;
}

/**
 * The word for a snapshot state, as the sidebar's `/devices` presence: a
 * state this build does not know reads as offline, never as connected.
 */
export function stateLabel(state) {
  switch (state) {
    case 'connected':
      return 'Connected';
    case 'paused':
      return 'Paused';
    default:
      return 'Offline';
  }
}

/** Read one `system:agent:status` payload; null without an `agents` array. */
export function parseSnapshot(payload) {
  const agents = payload && Array.isArray(payload.agents) ? payload.agents : null;
  if (!agents) {
    return null;
  }
  return {
    agents: agents.filter(
      (a) => a && typeof a === 'object' && typeof a.channel_id === 'string' && a.channel_id
    ),
    publishedMs: 0,
  };
}

/**
 * The snapshot to hold after one more delivery, or null to keep the current
 * one. Sticky replay delivers several full snapshots oldest-first; each
 * replaces the last entirely, and one published before the held one is
 * ignored (as the sidebar's next_paired_agents).
 */
export function nextSnapshot(current, payload, publishedMs) {
  const ts = Number(publishedMs) || 0;
  if (current && ts < current.publishedMs) {
    return null;
  }
  const next = parseSnapshot(payload);
  if (!next) {
    return null;
  }
  next.publishedMs = ts;
  return next;
}

function channelIds(snapshot) {
  return new Set((snapshot?.agents || []).map((a) => a.channel_id));
}

/**
 * Whether the set of channel ids differs between two snapshots, i.e. the
 * pairings (and their tiers) must be read again. No previous snapshot counts
 * as a change.
 */
export function channelSetChanged(prev, next) {
  if (!prev) {
    return true;
  }
  const a = channelIds(prev);
  const b = channelIds(next);
  if (a.size !== b.size) {
    return true;
  }
  for (const id of a) {
    if (!b.has(id)) {
      return true;
    }
  }
  return false;
}

/**
 * One card per agent pairing, in the daemon's pairing order.
 *
 * `guarded` comes ONLY from the snapshot (R39, ruling BF-2): it is null while
 * no snapshot names the pairing, and the switch is then disabled. A pairing
 * the snapshot does not name reads as offline.
 */
export function museRows(pairings, snapshot) {
  const byId = new Map(
    (snapshot?.agents || []).filter((a) => a && a.channel_id).map((a) => [a.channel_id, a])
  );
  return (Array.isArray(pairings) ? pairings : [])
    .filter((p) => p && p.kind === 'agent' && typeof p.control_channel_id === 'string')
    .map((p) => {
      const entry = byId.get(p.control_channel_id) || null;
      // The snapshot's label is what the sidebar row shows; the pairing's
      // own label covers an agent the snapshot does not name yet.
      const name = displayName(
        [entry?.label, p.label].find((l) => typeof l === 'string' && l.trim())
      );
      const state = entry && ['connected', 'paused'].includes(entry.state) ? entry.state : 'offline';
      const label = stateLabel(state);
      return {
        channelId: p.control_channel_id,
        name,
        state,
        stateLabel: label,
        heading: `${name} · ${label}`,
        tier: isTier(p.tier) ? p.tier : null,
        known: Boolean(entry),
        guarded: entry ? entry.muse_may_approve_guarded === true : null,
      };
    });
}

/** Whether a `remote.agent_set_guarded_approval` answer confirms `want`. */
export function confirmsGuarded(data, want) {
  return Boolean(data) && data.enabled === want;
}

/** The reason text when the daemon answered but did not confirm `want`. */
export function unconfirmedReason(want) {
  return `the daemon did not confirm the setting is ${want ? 'on' : 'off'}`;
}

export function tierError(message) {
  return `Could not change the tier: ${message}`;
}

export function settingError(message) {
  return `Could not change the setting: ${message}`;
}

export function loadError(message) {
  return `Could not load paired agents: ${message}`;
}
