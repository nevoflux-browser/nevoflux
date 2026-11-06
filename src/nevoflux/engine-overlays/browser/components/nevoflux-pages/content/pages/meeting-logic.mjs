/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * DOM-free logic for nevoflux://meeting and the Meetings settings section.
 *
 * A meeting is recorded by a pendant and kept by the daemon (`meeting/` in
 * nevoflux-agent). The page reads a meeting whole with `meeting.get`, then
 * follows it live on the EventBus:
 *
 *   system:meeting:state    sticky {live:[id], auto_open}
 *   system:meeting:partial  {meeting_id, seg, offset_ms, text, dropped?}
 *   system:meeting:entry    {meeting_id, entry}   a sentence's latest state
 *   system:meeting:ended    {meeting_id, status}
 *   system:meeting:summary  {meeting_id, summary | error}
 *
 * A sentence is first a partial (what is being said, grey), then an entry
 * with `raw` (said), then the same entry again with `clean` (tidied), which
 * replaces it in place. Tested by tests/unit/meeting-logic.test.mjs.
 */

export const TOPICS = Object.freeze({
  state: 'system:meeting:state',
  partial: 'system:meeting:partial',
  entry: 'system:meeting:entry',
  ended: 'system:meeting:ended',
  summary: 'system:meeting:summary',
});

export const TOPIC_PATTERNS = Object.freeze(Object.values(TOPICS));

export const AUDIO_DAYS_OPTIONS = Object.freeze([
  { value: 7, label: '7 days' },
  { value: 30, label: '30 days' },
  { value: 90, label: '90 days' },
  { value: null, label: 'Forever' },
]);

/** 65_000 → "01:05", 3_725_000 → "1:02:05". */
export function clock(ms) {
  const s = Math.max(0, Math.floor((Number(ms) || 0) / 1000));
  const two = (n) => String(n).padStart(2, '0');
  if (s >= 3600) return `${Math.floor(s / 3600)}:${two(Math.floor(s / 60) % 60)}:${two(s % 60)}`;
  return `${two(Math.floor(s / 60))}:${two(s % 60)}`;
}

/** "me" → "Me", "other" → "Others", "spk2" → "Speaker 2"; anything else as is. */
export function whoLabel(who) {
  if (who === 'me') return 'Me';
  if (who === 'other') return 'Others';
  const m = /^spk(\d+)$/.exec(who || '');
  return m ? `Speaker ${m[1]}` : who || 'Others';
}

/** The text a sentence shows: tidied once it is, as said until then. */
export function displayText(entry) {
  const clean = typeof entry?.clean === 'string' ? entry.clean.trim() : '';
  return clean || entry?.raw || '';
}

/** Still waiting for its tidy pass (the page shows it as said, marked). */
export function isCleaning(entry) {
  return entry?.kind === 'speech' && entry.clean == null && !entry.clean_failed;
}

/** A fresh view of one meeting. */
export function createView(id) {
  return { id, meta: null, entries: new Map(), partials: new Map(), summary: null, summaryError: null, live: false };
}

/** Take in a `meeting.get` answer; live events received before it are kept. */
export function applyGet(view, data) {
  view.meta = data?.meta || null;
  view.live = !!data?.live;
  view.summary = data?.summary || view.summary;
  for (const e of data?.transcript || []) {
    const had = view.entries.get(e.seg);
    // An event can be newer than the snapshot: a clean text never goes back.
    if (!had || !(had.clean != null && e.clean == null)) view.entries.set(e.seg, e);
    view.partials.delete(e.seg);
  }
  return view;
}

/**
 * Take in one EventBus event. Returns what changed, for the page to repaint
 * only that: 'row' (with seg), 'meta', 'summary', or null if it is not this
 * meeting's.
 */
export function applyEvent(view, ev) {
  const p = ev?.payload;
  if (!p || p.meeting_id !== view.id) return null;
  switch (ev.topic) {
    case TOPICS.partial: {
      if (view.entries.has(p.seg)) return null; // its final is already in
      if (p.dropped || !p.text) view.partials.delete(p.seg);
      else view.partials.set(p.seg, { seg: p.seg, offset_ms: p.offset_ms || 0, text: p.text });
      return { kind: 'row', seg: p.seg };
    }
    case TOPICS.entry: {
      if (!p.entry) return null;
      view.entries.set(p.entry.seg, p.entry);
      view.partials.delete(p.entry.seg);
      return { kind: 'row', seg: p.entry.seg };
    }
    case TOPICS.ended:
      view.live = false;
      if (view.meta) view.meta = { ...view.meta, status: p.status || 'ended' };
      return { kind: 'meta' };
    case TOPICS.summary:
      if (p.summary) {
        view.summary = p.summary;
        view.summaryError = null;
      } else {
        view.summaryError = p.error || 'The minutes could not be written';
      }
      return { kind: 'summary' };
    default:
      return null;
  }
}

/**
 * The transcript column, in time order: sentences said and being said, the
 * wearer's notes, and a marker for each span the pendant could not keep.
 * Questions go to the side panel instead.
 */
export function rows(view) {
  const out = [];
  for (const e of view.entries.values()) {
    if (e.kind === 'qa') continue;
    out.push({
      type: e.kind === 'note' ? 'note' : 'speech',
      seg: e.seg,
      offset_ms: e.offset_ms || 0,
      who: e.who,
      text: displayText(e),
      raw: e.raw,
      cleaning: isCleaning(e),
      tidied: displayText(e) !== e.raw,
      late: !!e.late,
    });
  }
  for (const p of view.partials.values()) {
    out.push({ type: 'partial', seg: p.seg, offset_ms: p.offset_ms, who: null, text: p.text });
  }
  for (const [from, to] of view.meta?.gaps || []) {
    out.push({ type: 'gap', seg: null, offset_ms: from, text: `${clock(to - from)} missing` });
  }
  out.sort((a, b) => a.offset_ms - b.offset_ms || (a.seg ?? -1) - (b.seg ?? -1));
  return out;
}

/** The questions asked during the meeting and their answers. */
export function questions(view) {
  return [...view.entries.values()]
    .filter((e) => e.kind === 'qa')
    .sort((a, b) => a.offset_ms - b.offset_ms)
    .map((e) => ({ seg: e.seg, offset_ms: e.offset_ms, question: e.raw, answer: e.clean || '' }));
}

/** The minutes as the side panel shows them, or null while there are none. */
export function minutesView(summary) {
  const m = summary?.minutes;
  if (!m) return null;
  return {
    title: m.title || 'Minutes',
    pass: summary.pass === 'final' ? 'final' : 'quick',
    topics: m.topics || [],
    decisions: m.decisions || [],
    todos: (m.todos || []).map((t) => ({
      owner: t.owner || 'Unassigned',
      task: t.task || '',
      due: t.due || '',
    })),
    openQuestions: m.open_questions || [],
    markdown: summary.markdown || '',
    inBrain: !!summary.brain_ref,
  };
}

/** The next speaker for a click on a sentence's label: me ⇄ others. */
export function toggledWho(who) {
  return who === 'me' ? 'other' : 'me';
}

/** One line of the history list. */
export function listingLine(row, liveIds = []) {
  const started = row?.started_at ? new Date(row.started_at) : null;
  const ended = row?.ended_at ? new Date(row.ended_at) : null;
  const live = liveIds.includes(row?.id);
  const length = started && ended ? clock(ended - started) : '';
  const when = started && !isNaN(started)
    ? started.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
    : row?.id || '';
  let status = '';
  if (live) status = 'Recording';
  else if (row?.status === 'aborted') status = 'Ended abruptly';
  else if (!row?.has_summary) status = 'No minutes';
  return { id: row?.id, when, length, sentences: row?.sentences || 0, live, status };
}

/** The line warning that text leaves the machine, or '' when it does not. */
export function cloudNotice(settingsData) {
  const names = [];
  // Tidying switched off sends nothing sentence by sentence.
  const keys = settingsData?.settings?.clean === false ? ['summary_model'] : ['clean_model', 'summary_model'];
  for (const key of keys) {
    const m = settingsData?.[key];
    if (m?.cloud && m.name && !names.includes(m.name)) names.push(m.name);
  }
  return names.length ? `Meeting text is sent to ${names.join(' and ')}` : '';
}

/** "provider/model" or "" (follow the agent) from what the settings form holds. */
export function modelValue(provider, model) {
  return provider ? (model ? `${provider}/${model}` : provider) : '';
}

/** Back from the form's "provider/model" (or "") to the two settings fields. */
export function splitModel(value) {
  const v = (value || '').trim();
  if (!v) return { provider: null, model: null };
  const i = v.indexOf('/');
  return i < 0 ? { provider: v, model: null } : { provider: v.slice(0, i), model: v.slice(i + 1) || null };
}
