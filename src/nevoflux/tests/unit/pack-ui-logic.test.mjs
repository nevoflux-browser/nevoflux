/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Unit tests for pack-ui-logic.mjs — the DOM-free logic backing the
 * Packs settings section. These mirror exactly the inline `_PackLogic`
 * helpers duplicated inside settings.js.
 */

import { describe, it, expect } from './test-runner.mjs';
import {
  packListToRows,
  validateResultMessage,
  installParams,
  uninstallParams,
  updateParams,
  packErrorMessage,
} from '../../engine-overlays/browser/components/nevoflux-pages/content/pages/pack-ui-logic.mjs';

describe('pack-ui-logic: packListToRows', () => {
  it('returns [] for an empty pack list', () => {
    expect(packListToRows({ packs: [] })).toEqual([]);
  });

  it('returns [] for nullish / garbage input', () => {
    expect(packListToRows(null)).toEqual([]);
    expect(packListToRows(undefined)).toEqual([]);
    expect(packListToRows({})).toEqual([]);
    expect(packListToRows(42)).toEqual([]);
    expect(packListToRows({ packs: 'nope' })).toEqual([]);
  });

  it('maps a single pack to a normalised row', () => {
    const rows = packListToRows({
      packs: [{ name: 'demo', version: '1.2.3', installed_at: '2026-06-10' }],
    });
    expect(rows).toEqual([
      { name: 'demo', version: '1.2.3', installedAt: '2026-06-10' },
    ]);
  });

  it('maps multiple packs preserving order', () => {
    const rows = packListToRows({
      packs: [
        { name: 'a', version: '0.1', installed_at: 't1' },
        { name: 'b', version: '0.2', installed_at: 't2' },
      ],
    });
    expect(rows.length).toBe(2);
    expect(rows[0].name).toBe('a');
    expect(rows[1].name).toBe('b');
    expect(rows[1].installedAt).toBe('t2');
  });

  it('accepts a bare array as well as { packs }', () => {
    const rows = packListToRows([{ name: 'x', version: '9' }]);
    expect(rows).toEqual([{ name: 'x', version: '9', installedAt: '' }]);
  });

  it('tolerates camelCase installedAt and missing fields', () => {
    const rows = packListToRows({
      packs: [{ name: 'y', installedAt: 'whenever' }],
    });
    expect(rows[0]).toEqual({ name: 'y', version: '', installedAt: 'whenever' });
  });

  it('coerces non-string version/installed_at to strings', () => {
    const rows = packListToRows({
      packs: [{ name: 'z', version: 3, installed_at: 1700000000 }],
    });
    expect(rows[0].version).toBe('3');
    expect(rows[0].installedAt).toBe('1700000000');
  });

  it('drops non-object entries', () => {
    const rows = packListToRows({ packs: [null, 'bad', { name: 'ok' }] });
    expect(rows.length).toBe(1);
    expect(rows[0].name).toBe('ok');
  });
});

describe('pack-ui-logic: validateResultMessage', () => {
  it('reports success for { ok: true }', () => {
    const msg = validateResultMessage({ ok: true });
    expect(msg).toContain('passed');
  });

  it('lists string violations when ok:false', () => {
    const msg = validateResultMessage({
      ok: false,
      violations: ['disallowed binary', 'network egress'],
    });
    expect(msg).toContain('Validation failed (2 violations)');
    expect(msg).toContain('disallowed binary');
    expect(msg).toContain('network egress');
  });

  it('uses singular noun for one violation', () => {
    const msg = validateResultMessage({ ok: false, violations: ['nope'] });
    expect(msg).toContain('(1 violation)');
    expect(msg).not.toContain('violations)');
  });

  it('extracts message field from object violations', () => {
    const msg = validateResultMessage({
      ok: false,
      violations: [{ message: 'bad rule', rule: 'R1' }],
    });
    expect(msg).toContain('bad rule');
  });

  it('falls back to rule field when no message', () => {
    const msg = validateResultMessage({
      ok: false,
      violations: [{ rule: 'R7' }],
    });
    expect(msg).toContain('R7');
  });

  it('handles ok:false with empty violations', () => {
    expect(validateResultMessage({ ok: false, violations: [] })).toBe(
      'Validation failed.'
    );
  });

  it('handles nullish result as failure', () => {
    expect(validateResultMessage(null)).toBe('Validation failed.');
    expect(validateResultMessage(undefined)).toBe('Validation failed.');
  });
});

describe('pack-ui-logic: installParams', () => {
  it('builds synchronous (wait:true) install params by default', () => {
    expect(installParams('/tmp/pack.toml')).toEqual({
      manifest_path: '/tmp/pack.toml',
      wait: true,
    });
  });

  it('trims the manifest path', () => {
    expect(installParams('  /a/b.toml  ').manifest_path).toBe('/a/b.toml');
  });

  it('supports wait:false (streaming) opt-in', () => {
    expect(installParams('/p.toml', { wait: false })).toEqual({
      manifest_path: '/p.toml',
      wait: false,
    });
  });

  it('adds force when requested', () => {
    expect(installParams('/p.toml', { force: true })).toEqual({
      manifest_path: '/p.toml',
      wait: true,
      force: true,
    });
  });

  it('coerces a non-string path to empty string', () => {
    expect(installParams(null).manifest_path).toBe('');
  });
});

describe('pack-ui-logic: uninstallParams', () => {
  it('defaults purge_data to false', () => {
    expect(uninstallParams('demo')).toEqual({
      name: 'demo',
      purge_data: false,
    });
  });

  it('sets purge_data when purgeData is true', () => {
    expect(uninstallParams('demo', { purgeData: true })).toEqual({
      name: 'demo',
      purge_data: true,
    });
  });

  it('does not set purge_data true for truthy-but-not-true values', () => {
    expect(uninstallParams('demo', { purgeData: 'yes' }).purge_data).toBe(false);
  });

  it('adds force when requested', () => {
    expect(uninstallParams('demo', { force: true })).toEqual({
      name: 'demo',
      purge_data: false,
      force: true,
    });
  });
});

describe('pack-ui-logic: updateParams', () => {
  it('builds trimmed manifest_path', () => {
    expect(updateParams('  /u.toml ')).toEqual({ manifest_path: '/u.toml' });
  });

  it('coerces non-string to empty string', () => {
    expect(updateParams(undefined)).toEqual({ manifest_path: '' });
  });
});

describe('pack-ui-logic: packErrorMessage', () => {
  it('special-cases KNOWLEDGE_UNSUPPORTED', () => {
    const msg = packErrorMessage({
      code: 'KNOWLEDGE_UNSUPPORTED',
      message: 'feature off',
    });
    expect(msg).toContain('not supported');
    expect(msg).toContain('feature off');
  });

  it('handles KNOWLEDGE_UNSUPPORTED with no message', () => {
    const msg = packErrorMessage({ code: 'KNOWLEDGE_UNSUPPORTED' });
    expect(msg).toContain('not supported');
  });

  it('labels INSTALL_FAILED and keeps detail', () => {
    const msg = packErrorMessage({
      code: 'INSTALL_FAILED',
      message: 'checksum mismatch',
    });
    expect(msg).toContain('Install failed');
    expect(msg).toContain('checksum mismatch');
  });

  it('labels UPDATE_FAILED / UNINSTALL_FAILED / VALIDATION_FAILED', () => {
    expect(packErrorMessage({ code: 'UPDATE_FAILED', message: 'x' })).toContain(
      'Update failed'
    );
    expect(
      packErrorMessage({ code: 'UNINSTALL_FAILED', message: 'y' })
    ).toContain('Uninstall failed');
    expect(
      packErrorMessage({ code: 'VALIDATION_FAILED', message: 'z' })
    ).toContain('Validation failed');
  });

  it('falls back to message when code is unknown', () => {
    expect(packErrorMessage({ code: 'WAT', message: 'boom' })).toBe('boom');
  });

  it('reads code/message from a nested error envelope', () => {
    const msg = packErrorMessage({
      error: { code: 'KNOWLEDGE_UNSUPPORTED', message: 'nested' },
    });
    expect(msg).toContain('not supported');
    expect(msg).toContain('nested');
  });

  it('handles a bare string', () => {
    expect(packErrorMessage('plain text')).toBe('plain text');
  });

  it('handles an Error instance', () => {
    expect(packErrorMessage(new Error('oops'))).toBe('oops');
  });

  it('handles null / empty', () => {
    expect(packErrorMessage(null)).toBe('Unknown error.');
    expect(packErrorMessage({})).toBe('Unknown error.');
  });
});
