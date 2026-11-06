/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/* global NevofluxPage */

/**
 * nevoflux://meeting[/<id>] — a pendant's meetings, the live one as it is
 * said. The DOM half; what a meeting looks like and how its events fold in
 * is meeting-logic.mjs.
 */
const Meeting = {
  L: null,
  view: null,
  live: [],
  listing: [],
  audio: null,

  async init() {
    this.L = await import('chrome://nevoflux/content/pages/meeting-logic.mjs');
    await this._subscribe();
    await this._loadList();
    const wanted = NevofluxPage.getParam('id');
    const first = wanted || this.live[0] || this.listing[0]?.id;
    if (first) await this.open(first);
    this._loadCloudNotice();
  },

  // ── Daemon ───────────────────────────────────────────

  async _call(command, params = {}) {
    const result = await NevofluxPage.sendQuery('bridge:request', {
      type: 'agent:command',
      payload: { command, params },
    });
    if (!result || !result.success) {
      throw new Error(result?.error?.message || result?.error || 'Bridge request failed');
    }
    const r = result.data;
    if (!r || !r.success) throw new Error(r?.error?.message || r?.error || 'Agent command failed');
    return r.data || {};
  },

  async _subscribe() {
    const channelId = 'meeting_' + Date.now() + '_' + Math.random().toString(36).slice(2, 8);
    await NevofluxPage.sendQuery('events:channel_open', { channelId });
    window.addEventListener('NevofluxMessage', (event) => {
      const d = event.detail;
      if (!d || d.type !== 'bridge:push' || d.msg?.type !== 'events:delivery') return;
      const ev = d.msg.payload?.event;
      if (ev) this._onEvent(ev);
    });
    const res = await NevofluxPage.sendQuery('bridge:request', {
      type: 'events.subscribe',
      payload: { patterns: this.L.TOPIC_PATTERNS, replay_sticky: true, channel_id: channelId },
    });
    if (!res || res.success === false) console.warn('[meeting] subscribe failed', res?.error);
  },

  _onEvent(ev) {
    if (ev.topic === this.L.TOPICS.state) {
      const before = this.live;
      this.live = Array.isArray(ev.payload?.live) ? ev.payload.live : [];
      const started = this.live.find((id) => !before.includes(id));
      this._loadList();
      // A meeting starting while the page is open is the one to watch.
      if (started && (!this.view || !this.view.live)) this.open(started);
      return;
    }
    if (!this.view) return;
    const change = this.L.applyEvent(this.view, ev);
    if (!change) return;
    if (change.kind === 'row') this._paintTranscript(change.seg);
    else if (change.kind === 'meta') {
      this._paintHeader();
      this._loadList();
    } else if (change.kind === 'summary') {
      this._paintSide();
      this._loadList();
    }
  },

  async _loadList() {
    try {
      const data = await this._call('meeting.list');
      this.listing = data.meetings || [];
      if (Array.isArray(data.live)) this.live = data.live;
    } catch (e) {
      this.listing = [];
      this._listError = e.message;
    }
    this._paintList();
  },

  async _loadCloudNotice() {
    const el = document.getElementById('meeting-cloud');
    try {
      const text = this.L.cloudNotice(await this._call('meeting.settings.get'));
      el.textContent = text;
      el.classList.toggle('shown', !!text);
    } catch (_e) {
      el.classList.remove('shown');
    }
  },

  async open(id) {
    this.view = this.L.createView(id);
    this._paintList();
    this._paintTranscript();
    try {
      this.L.applyGet(this.view, await this._call('meeting.get', { id }));
    } catch (e) {
      this._showEmpty(`Could not load this meeting: ${e.message}`);
      return;
    }
    this._paintHeader();
    this._paintTranscript();
    this._paintSide();
  },

  // ── Painting ─────────────────────────────────────────

  _el(tag, cls, text) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  },

  _paintList() {
    const box = document.getElementById('meeting-list-items');
    box.textContent = '';
    if (this._listError && !this.listing.length) {
      box.appendChild(this._el('div', 'side-note', `Could not load meetings: ${this._listError}`));
      return;
    }
    for (const row of this.listing) {
      const line = this.L.listingLine(row, this.live);
      const b = this._el('button', 'meeting-list-item');
      if (this.view?.id === line.id) b.classList.add('selected');
      if (line.live) b.appendChild(this._el('span', 'live-dot'));
      b.appendChild(document.createTextNode(line.when));
      const sub = [line.length, `${line.sentences} sentences`, line.status].filter(Boolean).join(' · ');
      b.appendChild(this._el('span', 'sub', sub));
      b.addEventListener('click', () => this.open(line.id));
      box.appendChild(b);
    }
  },

  _paintHeader() {
    const v = this.view;
    const title = this.L.minutesView(v.summary)?.title;
    const line = this.L.listingLine({ ...(v.meta || {}), id: v.id }, v.live ? [v.id] : []);
    document.getElementById('meeting-title').textContent = title || line.when || 'Meeting';
    document.getElementById('meeting-status').textContent = v.live ? 'Recording…' : line.status || line.length;
  },

  _showEmpty(text) {
    const box = document.getElementById('transcript');
    box.textContent = '';
    box.appendChild(this._el('div', 'empty', text));
  },

  /** Repaint the transcript; `seg` names the one row that changed (it fades in). */
  _paintTranscript(seg) {
    const box = document.getElementById('transcript');
    const rows = this.L.rows(this.view);
    if (!rows.length) {
      this._showEmpty(this.view.live ? 'Listening… sentences appear here as they are said.' : 'Nothing was said in this meeting.');
      return;
    }
    const stick = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
    box.textContent = '';
    for (const r of rows) box.appendChild(this._row(r, r.seg === seg && seg != null));
    if (stick) box.scrollTop = box.scrollHeight;
  },

  _row(r, fresh) {
    if (r.type === 'gap') return this._el('div', 'row gap', r.text);
    const row = this._el('div', `row ${r.type}`);
    if (r.who === 'me') row.classList.add('me');
    if (r.cleaning) row.classList.add('cleaning');
    if (fresh) row.classList.add('fresh');
    const meta = this._el('div', 'meta');
    meta.appendChild(this._el('span', null, this.L.clock(r.offset_ms)));
    if (r.type === 'speech') {
      const who = this._el('button', 'who', this.L.whoLabel(r.who));
      who.title = 'Click if this was said by someone else';
      who.addEventListener('click', () => this._setSpeaker(r));
      meta.appendChild(who);
      const play = this._el('button', 'play', '▶');
      play.title = 'Play what was said';
      play.addEventListener('click', () => this._play(r.seg));
      meta.appendChild(play);
      if (r.late) meta.appendChild(this._el('span', 'tag', 'sent late'));
    } else if (r.type === 'note') {
      meta.appendChild(this._el('span', 'tag', 'Note'));
    }
    row.appendChild(meta);
    const bubble = this._el('div', 'bubble', r.text);
    if (r.tidied) bubble.title = `As said: ${r.raw}`;
    row.appendChild(bubble);
    return row;
  },

  _paintSide() {
    const side = document.getElementById('meeting-side');
    side.textContent = '';
    const v = this.view;
    const m = this.L.minutesView(v.summary);
    if (m) {
      side.appendChild(this._el('h3', null, m.pass === 'final' ? 'Minutes' : 'Minutes (quick)'));
      const list = (name, items) => {
        if (!items.length) return;
        side.appendChild(this._el('h3', null, name));
        const ul = this._el('ul');
        for (const i of items) ul.appendChild(this._el('li', null, i));
        side.appendChild(ul);
      };
      list('Topics', m.topics);
      list('Decisions', m.decisions);
      list('To-dos', m.todos.map((t) => `${t.owner}: ${t.task}${t.due ? ` (${t.due})` : ''}`));
      list('Open questions', m.openQuestions);
      if (m.inBrain) side.appendChild(this._el('div', 'side-note', 'Saved to the knowledge base.'));
      const actions = this._el('div', 'side-actions');
      const copy = this._el('button', 'meeting-btn', 'Copy as Markdown');
      copy.addEventListener('click', () => navigator.clipboard.writeText(m.markdown));
      actions.appendChild(copy);
      side.appendChild(actions);
    } else if (v.summaryError) {
      side.appendChild(this._el('div', 'side-note', `The minutes could not be written: ${v.summaryError}`));
    } else if (!v.live) {
      side.appendChild(this._el('div', 'side-note', 'Writing the minutes…'));
    } else {
      side.appendChild(this._el('div', 'side-note', 'The minutes are written when the meeting ends.'));
    }

    const qs = this.L.questions(v);
    if (qs.length) {
      side.appendChild(this._el('h3', null, 'Questions asked'));
      for (const q of qs) {
        const d = this._el('div', 'qa');
        d.appendChild(this._el('div', 'q', `${this.L.clock(q.offset_ms)} ${q.question}`));
        d.appendChild(this._el('div', null, q.answer));
        side.appendChild(d);
      }
    }

    if (!v.live) {
      const actions = this._el('div', 'side-actions');
      const del = this._el('button', 'meeting-btn danger', 'Delete meeting');
      del.addEventListener('click', () => this._delete());
      actions.appendChild(del);
      side.appendChild(actions);
    }
  },

  // ── Actions ──────────────────────────────────────────

  async _setSpeaker(r) {
    try {
      await this._call('meeting.set_speaker', { id: this.view.id, seg: r.seg, who: this.L.toggledWho(r.who) });
    } catch (e) {
      console.warn('[meeting] set_speaker failed', e);
    }
  },

  async _play(seg) {
    try {
      const data = await this._call('meeting.audio', { id: this.view.id, seg });
      const bytes = Uint8Array.from(atob(data.base64), (c) => c.charCodeAt(0));
      if (this.audio) {
        this.audio.pause();
        URL.revokeObjectURL(this.audio.src);
      }
      this.audio = new Audio(URL.createObjectURL(new Blob([bytes], { type: data.mime })));
      await this.audio.play();
    } catch (e) {
      console.warn('[meeting] cannot play', e);
    }
  },

  async _delete() {
    // eslint-disable-next-line no-alert
    if (!window.confirm('Delete this meeting, its audio and its minutes (also from the knowledge base)?')) return;
    try {
      await this._call('meeting.delete', { id: this.view.id });
    } catch (e) {
      console.warn('[meeting] delete failed', e);
      return;
    }
    this.view = null;
    await this._loadList();
    const next = this.listing[0]?.id;
    if (next) await this.open(next);
    else {
      this._showEmpty('No meetings.');
      document.getElementById('meeting-side').textContent = '';
      document.getElementById('meeting-title').textContent = 'Meetings';
      document.getElementById('meeting-status').textContent = '';
    }
  },
};

document.addEventListener('DOMContentLoaded', () => {
  Meeting.init().catch((e) => {
    console.error('[meeting] init failed', e);
    Meeting._showEmpty(`Could not reach the agent: ${e.message}`);
  });
});
