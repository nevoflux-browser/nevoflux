/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Unit tests for local-inference-logic.mjs — the DOM-free logic backing the
 * On-device group on nevoflux://settings.
 *
 * The inputs here are the daemon's real wire shapes, copied from
 * `crates/daemon/src/local/`: `LocalState` is internally tagged on `state`,
 * `LocalError` on `code`, and `Fit` on `fit`, all snake_case.
 */

import { describe, it, expect } from './test-runner.mjs';
import {
  formatBytes,
  cardView,
  modelOptionLabel,
  insufficientText,
  consentSummary,
  needsExitConfirm,
  stateText,
} from '../../engine-overlays/browser/components/nevoflux-pages/content/pages/local-inference-logic.mjs';

const MB = 1024 * 1024;
const GB = 1024 * MB;

/** A `local.status` payload with the parts a test cares about. */
function status(state, extra = {}) {
  return {
    state,
    latched: false,
    config: { model: 'qwen3-8b', quant: 'Q4_K_M' },
    engine: null,
    model: null,
    activeProvider: null,
    ...extra,
  };
}

describe('local-inference-logic: formatBytes', () => {
  it('uses one decimal for gigabytes', () => {
    expect(formatBytes(2.3 * GB)).toBe('2.3 GB');
  });

  it('uses whole megabytes', () => {
    expect(formatBytes(183 * MB)).toBe('183 MB');
    expect(formatBytes(536 * MB)).toBe('536 MB');
  });

  it('falls back to kilobytes and bytes for small numbers', () => {
    expect(formatBytes(4 * 1024)).toBe('4 KB');
    expect(formatBytes(0)).toBe('0 B');
  });
});

describe('local-inference-logic: cardView', () => {
  it('offers setup when nothing is installed', () => {
    const card = cardView(status({ state: 'idle' }));
    expect(card.badge).toBe('Off');
    expect(card.actions.some((a) => a.id === 'setup')).toBeTruthy();
    expect(card.progress).toBeNull();
  });

  it('reports download progress as a percentage', () => {
    const card = cardView(
      status({ state: 'downloading_model', done: 45 * MB, total: 100 * MB })
    );
    expect(card.progress.pct).toBe(45);
    expect(card.progress.label).toContain('45%');
    expect(card.actions.some((a) => a.id === 'cancel')).toBeTruthy();
  });

  it('offers to take over when ready but not the default provider', () => {
    const card = cardView(
      status(
        {
          state: 'ready',
          backend: 'cuda',
          degraded: false,
          reason: null,
          ctx: 32768,
          gpu_layers: 36,
          layer_count: 36,
        },
        { activeProvider: 'Anthropic' }
      )
    );
    expect(card.badge).toBe('Ready');
    expect(card.subtitle).toContain('Currently using Anthropic');
    expect(card.actions.some((a) => a.id === 'set-default')).toBeTruthy();
  });

  it('does not offer to take over when it is already the default', () => {
    const card = cardView(
      status(
        {
          state: 'ready',
          backend: 'cuda',
          degraded: false,
          reason: null,
          ctx: 32768,
          gpu_layers: 36,
          layer_count: 36,
        },
        { activeProvider: 'local' }
      )
    );
    expect(card.actions.some((a) => a.id === 'set-default')).toBeFalsy();
  });

  it('surfaces why a ready engine is degraded', () => {
    const card = cardView(
      status({
        state: 'ready',
        backend: 'cpu',
        degraded: true,
        reason: 'CUDA crashed twice; fell back to CPU',
        ctx: 16384,
        gpu_layers: 0,
        layer_count: 36,
      })
    );
    expect(card.degradedNote).toContain('CUDA crashed twice');
  });

  it('mentions an available engine update without demanding it', () => {
    const card = cardView(
      status(
        { state: 'stopped', backend: 'cuda' },
        { engine: { tag: 'b10909', kind: 'windows-x64-cuda', bytes: 570 * MB, update: 'available' } }
      )
    );
    expect(card.updateNote).toBeTruthy();
    expect(card.actions.some((a) => a.id === 'update')).toBeTruthy();
  });

  it('treats a required update as blocking', () => {
    const card = cardView(
      status(
        { state: 'stopped', backend: 'cuda' },
        { engine: { tag: 'b9000', kind: 'windows-x64-cuda', bytes: 570 * MB, update: 'required' } }
      )
    );
    expect(card.updateNote).toContain('required');
  });

  it('offers repair when the engine files are damaged', () => {
    const card = cardView(
      status({
        state: 'failed',
        error: { code: 'engine_corrupt', detail: 'libggml.so hash mismatch' },
        retryable: false,
      })
    );
    expect(card.actions.some((a) => a.id === 'repair')).toBeTruthy();
  });

  it('does not offer repair for a failed security self-check', () => {
    const card = cardView(
      status({
        state: 'failed',
        error: { code: 'engine_insecure', detail: 'unexpected file in archive' },
        retryable: false,
      })
    );
    expect(card.subtitle).toContain('security');
    expect(card.actions.some((a) => a.id === 'repair')).toBeFalsy();
  });

  it('explains that another window holds the engine', () => {
    const card = cardView(
      status({ state: 'failed', error: { code: 'busy' }, retryable: true })
    );
    expect(card.subtitle).toContain('another');
    expect(card.actions.some((a) => a.id === 'retry')).toBeTruthy();
  });
});

describe('local-inference-logic: modelOptionLabel', () => {
  const model = {
    id: 'qwen3-8b',
    display_name: 'Qwen3 8B',
    fit: { fit: 'full_gpu', ctx: 32768 },
  };

  it('selects a model whose file is already present', () => {
    const o = modelOptionLabel(model, { bits: 'Q4_K_M', bytes: 5 * GB, state: 'present', have: 5 * GB });
    expect(o.action).toBe('select');
    expect(o.disabled).toBeFalsy();
  });

  it('downloads a model that is missing', () => {
    const o = modelOptionLabel(model, { bits: 'Q4_K_M', bytes: 5 * GB, state: 'missing', have: 0 });
    expect(o.action).toBe('download');
    expect(o.text).toContain('5.0 GB');
  });

  it('resumes a partial download and says how far it got', () => {
    const o = modelOptionLabel(model, { bits: 'Q4_K_M', bytes: 4 * GB, state: 'partial', have: 1 * GB });
    expect(o.action).toBe('resume');
    expect(o.text).toContain('25%');
  });

  it('disables a model this machine cannot run', () => {
    const tooBig = { ...model, fit: { fit: 'insufficient', reason: 'needs 9.1 GB, machine has 8.0 GB' } };
    const o = modelOptionLabel(tooBig, { bits: 'Q4_K_M', bytes: 5 * GB, state: 'missing', have: 0 });
    expect(o.disabled).toBeTruthy();
    expect(o.action).toBeNull();
  });
});

describe('local-inference-logic: insufficientText', () => {
  const catalog = [
    { id: 'qwen3-8b', display_name: 'Qwen3 8B', fit: { fit: 'insufficient', reason: 'x' } },
    { id: 'qwen3-4b-instruct-2507', display_name: 'Qwen3 4B Instruct (2507)', fit: { fit: 'insufficient', reason: 'x' } },
    { id: 'qwen3-1.7b', display_name: 'Qwen3 1.7B', fit: { fit: 'cpu_only', ctx: 16384 } },
  ];

  it('names a model that would actually fit', () => {
    const text = insufficientText({ fit: 'insufficient', reason: 'needs more VRAM' }, catalog);
    expect(text).toContain("can't run");
    expect(text).toContain('Qwen3 1.7B');
  });

  it('says so plainly when nothing in the catalog fits', () => {
    const none = catalog.map((m) => ({ ...m, fit: { fit: 'insufficient', reason: 'x' } }));
    const text = insufficientText({ fit: 'insufficient', reason: 'needs more VRAM' }, none);
    expect(text).toContain("can't run");
    expect(text).toContain('needs more VRAM');
  });
});

describe('local-inference-logic: consentSummary', () => {
  const plan = {
    engine_bytes: 570 * MB,
    model_bytes: 5 * GB,
    backend: 'cuda',
    alternatives: [{ backend: 'vulkan', engine_bytes: 34 * MB }],
    sources: ['github.com', 'huggingface.co'],
    disk: { needed: 12 * GB, available: 200 * GB },
  };

  it('itemises what will be downloaded', () => {
    const s = consentSummary(plan);
    expect(s.lines.length).toBe(2);
    expect(s.total).toBe(570 * MB + 5 * GB);
    expect(s.sources).toEqual(['github.com', 'huggingface.co']);
  });

  it('offers the cheaper backend and says what it saves', () => {
    const s = consentSummary(plan);
    expect(s.altButton.label).toBe('Use Vulkan instead, save 536 MB');
    expect(s.altButton.backend).toBe('vulkan');
  });

  it('offers no alternative when there is none, or none cheaper', () => {
    expect(consentSummary({ ...plan, alternatives: [] }).altButton).toBeNull();
    const dearer = { ...plan, alternatives: [{ backend: 'cpu', engine_bytes: 900 * MB }] };
    expect(consentSummary(dearer).altButton).toBeNull();
  });

  it('reports the disk requirement', () => {
    const s = consentSummary(plan);
    expect(s.disk.needed).toBe(12 * GB);
    expect(s.disk.enough).toBeTruthy();
    expect(consentSummary({ ...plan, disk: { needed: 12 * GB, available: 1 * GB } }).disk.enough).toBeFalsy();
  });
});

describe('local-inference-logic: needsExitConfirm', () => {
  it('confirms only when leaving local while the latch is on', () => {
    expect(needsExitConfirm(true, 'anthropic')).toBeTruthy();
    expect(needsExitConfirm(true, 'local')).toBeFalsy();
    expect(needsExitConfirm(false, 'anthropic')).toBeFalsy();
  });
});

describe('local-inference-logic: stateText', () => {
  it('has a line for every state the daemon can publish', () => {
    const states = [
      { state: 'idle' },
      { state: 'probing' },
      { state: 'estimating_fit' },
      { state: 'awaiting_consent', engine_bytes: 1, model_bytes: 2, backend: 'cuda', sources: [] },
      { state: 'downloading_engine', done: 1, total: 2 },
      { state: 'installing_engine' },
      { state: 'verifying_engine' },
      { state: 'downloading_model', done: 1, total: 2 },
      { state: 'engine_starting' },
      { state: 'ready', backend: 'cuda', degraded: false, reason: null, ctx: 32768, gpu_layers: 36, layer_count: 36 },
      { state: 'stopped', backend: 'cuda' },
      { state: 'failed', error: { code: 'offline' }, retryable: true },
    ];
    for (const s of states) {
      const text = stateText(s);
      expect(typeof text).toBe('string');
      expect(text.length).toBeGreaterThan(0);
    }
  });

  it('does not say "undefined" for a state it does not know', () => {
    expect(stateText({ state: 'something_new' })).toContain('something_new');
  });
});
