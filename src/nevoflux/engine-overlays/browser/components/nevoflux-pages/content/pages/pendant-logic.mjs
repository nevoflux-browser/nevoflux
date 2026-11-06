/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * DOM-free logic for the Pendant section of nevoflux://settings: pairing a
 * NevoFlux pendant plugged in over USB, changing its Wi-Fi, listing the ones
 * paired here. The work is the daemon's (`pendant.*`, nevoflux-agent
 * `pendant/`); a job's progress arrives on the sticky EventBus topic
 * `system:pendant:job`:
 *
 *   probing → authorize → pairing → confirm → writing → replacing → waiting → done
 *                                                           (or failed / cancelled)
 *
 * Tested by tests/unit/pendant-logic.test.mjs.
 */

export const JOB_TOPIC = 'system:pendant:job';

const PHASE_TEXT = {
  probing: 'Talking to the pendant…',
  authorize: 'Approve the pendant in the page that just opened',
  pairing: 'Creating the pairing…',
  confirm: 'Press BOOT on the pendant',
  writing: 'Writing to the pendant…',
  replacing: 'Replacing its old pairing…',
  waiting: 'Waiting for the pendant to connect…',
};

const PAIR_STEPS = ['probing', 'authorize', 'pairing', 'confirm', 'writing', 'waiting'];
const WIFI_STEPS = ['probing', 'confirm', 'writing'];

export function isTerminal(phase) {
  return phase === 'done' || phase === 'failed' || phase === 'cancelled';
}

/** The steps a job of this kind shows, and how far `phase` is along them. */
export function progress(kind, phase) {
  const steps = kind === 'wifi' ? WIFI_STEPS : PAIR_STEPS;
  const at = phase === 'replacing' ? steps.indexOf('writing') : steps.indexOf(phase);
  return { steps: steps.length, done: phase === 'done' ? steps.length : Math.max(0, at) };
}

/** The line a job event shows. */
export function phaseText(ev) {
  if (!ev) return '';
  switch (ev.phase) {
    case 'done':
      if (ev.kind === 'wifi' || ev.wifi) return `Wi-Fi updated on ${ev.name || 'the pendant'}. It reconnects on its own.`;
      return ev.link
        ? `${ev.name || 'The pendant'} is paired and connected (${linkLabel(ev.link)}).`
        : `${ev.name || 'The pendant'} is paired. It connects once it is on Wi-Fi.`;
    case 'failed':
      return `Could not finish: ${ev.reason || 'something went wrong'}`;
    case 'cancelled':
      return 'Cancelled. Nothing was changed on the pendant.';
    default:
      return PHASE_TEXT[ev.phase] || ev.phase || '';
  }
}

/** How a paired pendant reaches this computer. */
export function linkLabel(link) {
  return { lan: 'LAN', relay: 'Relay', ble: 'Bluetooth' }[link] || 'Offline';
}

/** One entry of the port picker. */
export function portOption(row) {
  if (!row) return { value: '', label: '', usable: false };
  if (row.busy) return { value: row.port, label: `${row.port} — busy`, usable: false };
  if (row.error) return { value: row.port, label: `${row.port} — not answering`, usable: false };
  const name = row.name || 'NevoFlux pendant';
  const note = row.paired_here ? ' · paired here' : row.paired ? ' · paired elsewhere' : '';
  return { value: row.port, label: `${name} (${row.port})${note}`, usable: true };
}

/** The port to preselect: the only usable one, else the first usable. */
export function defaultPort(rows) {
  const usable = (rows || []).filter((r) => portOption(r).usable);
  return usable[0]?.port || '';
}

/** What a pairing on this port will do to an existing one. */
export function replaceNotice(row) {
  if (row?.paired_here) return 'This pendant is already paired here; pairing again replaces that pairing.';
  if (row?.paired) return 'This pendant is paired with another computer; that pairing stops working.';
  return '';
}

/** "HomeNet-5G" / "HomeNet_5G" / "HomeNet 5G" → "homenet": the name both bands usually share. */
function baseName(ssid) {
  return (ssid || '')
    .toLowerCase()
    .replace(/[\s_-]*(5g|5ghz|5\.0g|6g|6e)$/i, '')
    .replace(/[\s_-]*(2\.4g|2g|2\.4ghz)$/i, '');
}

/**
 * The Wi-Fi form's starting point from `pendant.wifi`: the computer's own
 * network when the pendant can join it (2.4 GHz), else a warning and, when the
 * same router has one, its 2.4 GHz name.
 */
export function wifiStart(view) {
  const cur = view?.current;
  const nearby = (view?.nearby || []).map((n) => n.ssid);
  if (!cur) return { ssid: '', password: '', warning: '', choices: nearby };
  if (cur.band === '2.4' || cur.band == null) {
    return { ssid: cur.ssid, password: view.password || '', warning: '', choices: nearby };
  }
  const sibling = nearby.find((s) => baseName(s) === baseName(cur.ssid) && s !== cur.ssid) || '';
  return {
    ssid: sibling,
    // The same router usually keeps one password for both bands.
    password: sibling ? view.password || '' : '',
    warning:
      `This computer is on ${cur.ssid} (${cur.band} GHz). The pendant only joins 2.4 GHz networks` +
      (sibling ? `; ${sibling} looks like the same router's 2.4 GHz name.` : '; pick a 2.4 GHz network below.'),
    choices: nearby,
  };
}

/** Whether a job event belongs to the job this page started (or, with no job, any). */
export function isOurs(ev, jobId) {
  return !!ev && (!jobId || ev.job === jobId);
}

/** One row of the paired list. */
export function pendantRow(p) {
  const when = p?.paired_at ? new Date(p.paired_at * 1000) : null;
  return {
    id: p?.control_channel_id,
    name: p?.name || 'NevoFlux pendant',
    status: p?.online ? `Connected · ${linkLabel(p.link)}` : 'Offline',
    online: !!p?.online,
    since: when && !isNaN(when) ? when.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' }) : '',
  };
}
