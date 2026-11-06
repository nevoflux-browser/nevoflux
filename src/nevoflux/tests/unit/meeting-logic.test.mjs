/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Unit tests for meeting-logic.mjs — the DOM-free logic behind
 * nevoflux://meeting. Inputs are the daemon's `meeting.get` answer and its
 * `system:meeting:*` events.
 */

import { describe, it, expect } from './test-runner.mjs';
import {
  TOPICS,
  clock,
  whoLabel,
  displayText,
  createView,
  applyGet,
  applyEvent,
  rows,
  questions,
  minutesView,
  toggledWho,
  listingLine,
  cloudNotice,
  modelValue,
  splitModel,
} from '../../engine-overlays/browser/components/nevoflux-pages/content/pages/meeting-logic.mjs';

const speech = (seg, offset_ms, raw, extra = {}) => ({ seg, offset_ms, kind: 'speech', who: 'other', raw, ...extra });
const ev = (topic, payload) => ({ topic, payload: { meeting_id: '7', ...payload } });

describe('meeting-logic: formatting', () => {
  it('clocks minutes and hours', () => {
    expect(clock(0)).toBe('00:00');
    expect(clock(65_000)).toBe('01:05');
    expect(clock(3_725_000)).toBe('1:02:05');
    expect(clock(-5)).toBe('00:00');
  });

  it('names speakers', () => {
    expect(whoLabel('me')).toBe('Me');
    expect(whoLabel('other')).toBe('Others');
    expect(whoLabel('spk2')).toBe('Speaker 2');
    expect(toggledWho('me')).toBe('other');
    expect(toggledWho('spk1')).toBe('me');
  });

  it('shows the tidied text once there is one', () => {
    expect(displayText(speech(0, 0, '嗯那个预算'))).toBe('嗯那个预算');
    expect(displayText(speech(0, 0, '嗯那个预算', { clean: '预算。' }))).toBe('预算。');
    // Tidied to nothing (only filler): what was said still shows.
    expect(displayText(speech(0, 0, '嗯', { clean: '' }))).toBe('嗯');
  });
});

describe('meeting-logic: live transcript', () => {
  it('grows a partial into a sentence and then tidies it in place', () => {
    const v = createView('7');
    expect(applyEvent(v, ev(TOPICS.partial, { seg: 0, offset_ms: 1000, text: '嗯那个' }))).toEqual({ kind: 'row', seg: 0 });
    let r = rows(v);
    expect(r.length).toBe(1);
    expect(r[0].type).toBe('partial');

    applyEvent(v, ev(TOPICS.entry, { entry: speech(0, 1000, '嗯那个预算要再确认') }));
    r = rows(v);
    expect(r.length).toBe(1);
    expect(r[0].type).toBe('speech');
    expect(r[0].cleaning).toBe(true);

    applyEvent(v, ev(TOPICS.entry, { entry: speech(0, 1000, '嗯那个预算要再确认', { clean: '预算要再确认。' }) }));
    r = rows(v);
    expect(r[0].text).toBe('预算要再确认。');
    expect(r[0].cleaning).toBe(false);
    expect(r[0].tidied).toBe(true);
    expect(r[0].raw).toBe('嗯那个预算要再确认');

    // A partial arriving after its final changes nothing.
    expect(applyEvent(v, ev(TOPICS.partial, { seg: 0, text: '嗯' }))).toBeNull();
  });

  it('drops a partial that turned out to be noise', () => {
    const v = createView('7');
    applyEvent(v, ev(TOPICS.partial, { seg: 3, text: '啊' }));
    applyEvent(v, ev(TOPICS.partial, { seg: 3, text: '', dropped: true }));
    expect(rows(v).length).toBe(0);
  });

  it('ignores other meetings', () => {
    const v = createView('8');
    expect(applyEvent(v, ev(TOPICS.entry, { entry: speech(0, 0, 'x') }))).toBeNull();
  });

  it('orders by time, marks late sentences and holes, keeps questions aside', () => {
    const v = createView('7');
    applyGet(v, {
      meta: { id: '7', status: 'live', gaps: [[60_000, 180_000]] },
      live: true,
      transcript: [
        speech(2, 200_000, '补传的一句', { late: true }),
        speech(0, 1_000, '开始', { clean: '开始。' }),
        { seg: 1, offset_ms: 50_000, kind: 'note', who: 'me', raw: '标记一下，预算' },
        { seg: 3, offset_ms: 210_000, kind: 'qa', who: 'me', raw: '几号？', clean: '周五。' },
      ],
    });
    const r = rows(v);
    expect(r.map((x) => x.type)).toEqual(['speech', 'note', 'gap', 'speech']);
    expect(r[2].text).toBe('02:00 missing');
    expect(r[3].late).toBe(true);
    expect(questions(v)).toEqual([{ seg: 3, offset_ms: 210_000, question: '几号？', answer: '周五。' }]);
    expect(v.live).toBe(true);
  });

  it('never lets a snapshot undo a newer tidy event', () => {
    const v = createView('7');
    applyEvent(v, ev(TOPICS.entry, { entry: speech(0, 0, '嗯好', { clean: '好。' }) }));
    applyGet(v, { meta: { id: '7' }, transcript: [speech(0, 0, '嗯好')] });
    expect(rows(v)[0].text).toBe('好。');
  });

  it('follows the end and the minutes', () => {
    const v = createView('7');
    applyGet(v, { meta: { id: '7', status: 'live' }, live: true, transcript: [] });
    expect(applyEvent(v, ev(TOPICS.ended, { status: 'ended' }))).toEqual({ kind: 'meta' });
    expect(v.live).toBe(false);
    expect(v.meta.status).toBe('ended');
    applyEvent(v, ev(TOPICS.summary, { error: 'no model' }));
    expect(v.summaryError).toBe('no model');
    applyEvent(v, ev(TOPICS.summary, { summary: { pass: 'quick', minutes: { title: '预算会' } } }));
    expect(v.summaryError).toBeNull();
    expect(minutesView(v.summary).title).toBe('预算会');
  });
});

describe('meeting-logic: minutes, history, settings', () => {
  it('fills the gaps a model leaves in the minutes', () => {
    const m = minutesView({
      pass: 'quick',
      brain_ref: 'meetings/2026-11-06-7',
      markdown: '# x',
      minutes: { todos: [{ task: '确认预算', owner: '', due: '' }] },
    });
    expect(m.title).toBe('Minutes');
    expect(m.todos[0].owner).toBe('Unassigned');
    expect(m.inBrain).toBe(true);
    expect(m.topics).toEqual([]);
    expect(minutesView(null)).toBeNull();
  });

  it('says which meetings are recording or ended oddly', () => {
    const base = { id: '7', started_at: '2026-11-06T06:30:00Z', sentences: 12 };
    expect(listingLine({ ...base, status: 'live' }, ['7']).status).toBe('Recording');
    const done = listingLine({ ...base, status: 'ended', ended_at: '2026-11-06T07:15:30Z', has_summary: true });
    expect(done.length).toBe('45:30');
    expect(done.status).toBe('');
    expect(listingLine({ ...base, status: 'aborted' }).status).toBe('Ended abruptly');
    expect(listingLine({ ...base, status: 'ended' }).status).toBe('No minutes');
  });

  it('warns only about models off this machine, once each', () => {
    expect(cloudNotice({ clean_model: { name: 'ollama/qwen', cloud: false }, summary_model: { name: 'ollama/qwen', cloud: false } })).toBe('');
    expect(cloudNotice({ clean_model: { name: 'openai/gpt', cloud: true }, summary_model: { name: 'openai/gpt', cloud: true } })).toBe(
      'Meeting text is sent to openai/gpt'
    );
    expect(
      cloudNotice({ settings: { clean: false }, clean_model: { name: 'a/x', cloud: true }, summary_model: { name: 'b/y', cloud: true } })
    ).toBe('Meeting text is sent to b/y');
    expect(cloudNotice({ clean_model: { error: 'no provider' } })).toBe('');
  });

  it('round-trips a model choice', () => {
    expect(modelValue(null, null)).toBe('');
    expect(modelValue('openai', 'gpt-5')).toBe('openai/gpt-5');
    expect(splitModel('')).toEqual({ provider: null, model: null });
    expect(splitModel('openai/gpt-5')).toEqual({ provider: 'openai', model: 'gpt-5' });
    expect(splitModel('custom:abc/my/model')).toEqual({ provider: 'custom:abc', model: 'my/model' });
    expect(splitModel('ollama')).toEqual({ provider: 'ollama', model: null });
  });
});
