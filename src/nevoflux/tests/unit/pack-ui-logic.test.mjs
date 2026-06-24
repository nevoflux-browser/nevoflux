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
  isRemoteSource,
  inspectParams,
  summarizeInspect,
  summarizePackProgress,
  parsePackInstallSrc,
  findInstalledVersion,
  comparePackVersions,
  decidePackAction,
} from '../../engine-overlays/browser/components/nevoflux-pages/content/pages/pack-ui-logic.mjs';

describe('pack-ui-logic: isRemoteSource', () => {
  it('treats github: shorthand as remote', () => {
    expect(isRemoteSource('github:user/repo')).toBe(true);
    expect(isRemoteSource('github:user/repo/sub@v1')).toBe(true);
  });

  it('treats https github URLs as remote', () => {
    expect(isRemoteSource('https://github.com/user/repo')).toBe(true);
  });

  it('tolerates surrounding whitespace', () => {
    expect(isRemoteSource('  github:user/repo  ')).toBe(true);
    expect(isRemoteSource('  https://github.com/u/r ')).toBe(true);
  });

  it('treats local paths as not remote', () => {
    expect(isRemoteSource('/tmp/pack.toml')).toBe(false);
    expect(isRemoteSource('./relative/manifest.toml')).toBe(false);
    expect(isRemoteSource('C:\\packs\\manifest.toml')).toBe(false);
  });

  it('treats non-github URLs as not remote', () => {
    expect(isRemoteSource('https://gitlab.com/u/r')).toBe(false);
    expect(isRemoteSource('https://example.com/github.com/u/r')).toBe(false);
  });

  it('treats empty / nullish / non-string as not remote', () => {
    expect(isRemoteSource('')).toBe(false);
    expect(isRemoteSource('   ')).toBe(false);
    expect(isRemoteSource(null)).toBe(false);
    expect(isRemoteSource(undefined)).toBe(false);
    expect(isRemoteSource(42)).toBe(false);
  });
});

describe('pack-ui-logic: inspectParams', () => {
  it('wraps the source in { source }', () => {
    expect(inspectParams('github:u/r@v1')).toEqual({ source: 'github:u/r@v1' });
  });

  it('trims the source', () => {
    expect(inspectParams('  github:u/r  ')).toEqual({ source: 'github:u/r' });
  });

  it('coerces a non-string source to empty string', () => {
    expect(inspectParams(null)).toEqual({ source: '' });
  });
});

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

  it('sends { source } for a github: remote source', () => {
    expect(installParams('github:u/r@v1')).toEqual({
      source: 'github:u/r@v1',
      wait: true,
    });
  });

  it('sends { source } for an https github URL', () => {
    expect(installParams('https://github.com/u/r')).toEqual({
      source: 'https://github.com/u/r',
      wait: true,
    });
  });

  it('does not include manifest_path for a remote source', () => {
    const params = installParams('github:u/r');
    expect(params.manifest_path).toBeUndefined();
    expect(params.source).toBe('github:u/r');
  });

  it('trims a remote source and honours force', () => {
    expect(installParams('  github:u/r  ', { force: true })).toEqual({
      source: 'github:u/r',
      wait: true,
      force: true,
    });
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
  it('builds trimmed manifest_path for a local path', () => {
    expect(updateParams('  /u.toml ')).toEqual({ manifest_path: '/u.toml' });
  });

  it('coerces non-string to empty manifest_path', () => {
    expect(updateParams(undefined)).toEqual({ manifest_path: '' });
  });

  it('sends { source } for a github: remote source', () => {
    expect(updateParams('github:u/r@v2')).toEqual({ source: 'github:u/r@v2' });
  });

  it('sends { source } for an https github URL (trimmed)', () => {
    expect(updateParams('  https://github.com/u/r ')).toEqual({
      source: 'https://github.com/u/r',
    });
  });
});

describe('pack-ui-logic: summarizeInspect', () => {
  const fullData = {
    source: 'github:u/r@v1',
    tarball_sha256: 'abc',
    pack: {
      name: 'demo',
      version: '0.1.0',
      description: 'A demo pack',
      authors: ['Sam'],
    },
    components: {
      skills: ['a', 'b'],
      canvas_tools: [{ name: 'pdf.render', binary: 'weasyprint' }],
      seed: ['ns/cv'],
      dashboard: 'ns-dashboard',
      knowledge: false,
    },
    violations: [],
  };

  it('renders the identity line with name, version, description', () => {
    const { text } = summarizeInspect(fullData);
    expect(text).toContain('demo 0.1.0 — A demo pack');
  });

  it('lists skills', () => {
    expect(summarizeInspect(fullData).text).toContain('Skills: a, b');
  });

  it('surfaces canvas-tool name and the binary it runs', () => {
    const { text } = summarizeInspect(fullData);
    expect(text).toContain('Canvas tools: pdf.render (runs: weasyprint)');
    expect(text).toContain('run the binaries');
  });

  it('lists seed pages and dashboard', () => {
    const { text } = summarizeInspect(fullData);
    expect(text).toContain('Seed pages: ns/cv');
    expect(text).toContain('Dashboard: ns-dashboard');
  });

  it('shows "<none>" when there are no violations', () => {
    const result = summarizeInspect(fullData);
    expect(result.text).toContain('Violations: <none>');
    expect(result.hasViolations).toBe(false);
    expect(result.violations).toEqual([]);
  });

  it('surfaces violations prominently and flags hasViolations', () => {
    const result = summarizeInspect({
      ...fullData,
      violations: ['SeedNotProtected { ns/cv }'],
    });
    expect(result.hasViolations).toBe(true);
    expect(result.violations).toEqual(['SeedNotProtected { ns/cv }']);
    expect(result.text).toContain('⚠ Violations (1 violation)');
    expect(result.text).toContain('SeedNotProtected { ns/cv }');
  });

  it('uses plural noun for multiple violations', () => {
    const result = summarizeInspect({
      ...fullData,
      violations: ['one', { message: 'two' }],
    });
    expect(result.text).toContain('⚠ Violations (2 violations)');
    expect(result.text).toContain('two');
  });

  it('shows knowledge when present', () => {
    const result = summarizeInspect({
      ...fullData,
      components: { ...fullData.components, knowledge: true },
    });
    expect(result.text).toContain('Knowledge: yes');
  });

  it('omits empty component sections gracefully', () => {
    const result = summarizeInspect({
      pack: { name: 'bare', version: '1.0.0' },
      components: {},
      violations: [],
    });
    expect(result.text).toContain('bare 1.0.0');
    expect(result.text).not.toContain('Skills:');
    expect(result.text).not.toContain('Canvas tools:');
    expect(result.text).toContain('Violations: <none>');
  });

  it('tolerates nullish / garbage input', () => {
    const result = summarizeInspect(null);
    expect(result.text).toContain('(unnamed pack)');
    expect(result.hasViolations).toBe(false);
  });

  it('renders a plain-string canvas tool without a binary', () => {
    const result = summarizeInspect({
      pack: { name: 'p', version: '1' },
      components: { canvas_tools: ['plain.tool'] },
      violations: [],
    });
    expect(result.text).toContain('Canvas tools: plain.tool');
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

describe('pack-ui-logic: summarizePackProgress', () => {
  it('summarizes a Running frame for the op as in-progress', () => {
    const v = summarizePackProgress(
      { op_id: 'op1', phase: 'Seed', status: 'Running', progress_pct: 40, log: 'seeding pages' },
      'op1'
    );
    expect(v.matched).toBe(true);
    expect(v.pct).toBe(40);
    expect(v.phase).toBe('Seed');
    expect(v.terminal).toBe(false);
  });

  it('marks an Ok frame as terminal success', () => {
    const v = summarizePackProgress(
      { op_id: 'op1', phase: 'Commit', status: 'Ok', progress_pct: 100, log: 'installed' },
      'op1'
    );
    expect(v.terminal).toBe(true);
    expect(v.ok).toBe(true);
    expect(v.failed).toBe(false);
  });

  it('builds a display line from phase, pct and log', () => {
    const v = summarizePackProgress(
      { op_id: 'op1', phase: 'Seed', status: 'Running', progress_pct: 65, log: 'seeding pages' },
      'op1'
    );
    expect(v.line).toBe('[Seed 65%] seeding pages');
  });

  it('treats a frame with missing/unknown status as non-terminal (defensive)', () => {
    const v = summarizePackProgress({ op_id: 'op1', phase: 'Seed', progress_pct: 10 }, 'op1');
    expect(v.terminal).toBe(false);
    expect(v.ok).toBe(false);
    expect(v.failed).toBe(false);
    expect(v.pct).toBe(10);
  });

  it('ignores a frame whose op_id does not match', () => {
    const v = summarizePackProgress(
      { op_id: 'other', phase: 'Seed', status: 'Running', progress_pct: 5, log: 'x' },
      'op1'
    );
    expect(v.matched).toBe(false);
  });

  it('marks RolledBack and Cancelled as terminal failures', () => {
    for (const status of ['Failed', 'RolledBack', 'Cancelled']) {
      const v = summarizePackProgress({ op_id: 'op1', phase: 'Commit', status, progress_pct: 100, log: 'x' }, 'op1');
      expect(v.terminal).toBe(true);
      expect(v.failed).toBe(true);
      expect(v.ok).toBe(false);
    }
  });
});

describe('pack-ui-logic: parsePackInstallSrc', () => {
  it('accepts github: shorthand and normalizes', () => {
    expect(parsePackInstallSrc('github:owner/repo')).toEqual({
      ok: true, source: 'github:owner/repo', display: 'owner/repo',
    });
    expect(parsePackInstallSrc('github:owner/repo@v1.2.0')).toEqual({
      ok: true, source: 'github:owner/repo@v1.2.0', display: 'owner/repo@v1.2.0',
    });
    expect(parsePackInstallSrc('github:owner/repo/sub/dir@v1')).toEqual({
      ok: true, source: 'github:owner/repo/sub/dir@v1', display: 'owner/repo/sub/dir@v1',
    });
  });

  it('accepts https github URLs and normalizes to github: form', () => {
    expect(parsePackInstallSrc('https://github.com/owner/repo')).toEqual({
      ok: true, source: 'github:owner/repo', display: 'owner/repo',
    });
    expect(parsePackInstallSrc('https://github.com/owner/repo/tree/v1/sub/dir')).toEqual({
      ok: true, source: 'github:owner/repo/sub/dir@v1', display: 'owner/repo/sub/dir@v1',
    });
  });

  it('trims whitespace', () => {
    expect(parsePackInstallSrc('  github:owner/repo  ').ok).toBe(true);
  });

  it('rejects missing / empty / non-string', () => {
    expect(parsePackInstallSrc('').ok).toBe(false);
    expect(parsePackInstallSrc('   ').ok).toBe(false);
    expect(parsePackInstallSrc(null).ok).toBe(false);
    expect(parsePackInstallSrc(42).ok).toBe(false);
  });

  it('rejects non-github and dangerous sources', () => {
    expect(parsePackInstallSrc('/tmp/pack.toml').ok).toBe(false);
    expect(parsePackInstallSrc('./rel/manifest.toml').ok).toBe(false);
    expect(parsePackInstallSrc('file:///etc/passwd').ok).toBe(false);
    expect(parsePackInstallSrc('javascript:alert(1)').ok).toBe(false);
    expect(parsePackInstallSrc('https://gitlab.com/u/r').ok).toBe(false);
    expect(parsePackInstallSrc('https://example.com/github.com/u/r').ok).toBe(false);
    expect(parsePackInstallSrc('git@github.com:u/r.git').ok).toBe(false);
  });

  it('rejects path traversal and control chars', () => {
    expect(parsePackInstallSrc('github:owner/repo/../../x').ok).toBe(false);
    expect(parsePackInstallSrc('github:owner/repo@v1\n').ok).toBe(false);
    expect(parsePackInstallSrc('github:owner/repo@..').ok).toBe(false);
  });

  it('rejects overlong input', () => {
    expect(parsePackInstallSrc('github:owner/' + 'r'.repeat(600)).ok).toBe(false);
  });
});

describe('pack-ui-logic: findInstalledVersion', () => {
  const rows = [
    { name: 'alpha', version: '1.0.0' },
    { name: 'beta', version: '2.3.1' },
  ];
  it('returns the version of a matching installed pack', () => {
    expect(findInstalledVersion(rows, 'beta')).toBe('2.3.1');
  });
  it('returns null when not installed or bad input', () => {
    expect(findInstalledVersion(rows, 'gamma')).toBe(null);
    expect(findInstalledVersion(null, 'beta')).toBe(null);
    expect(findInstalledVersion(rows, '')).toBe(null);
  });
});

describe('pack-ui-logic: comparePackVersions', () => {
  it('orders semver-ish versions', () => {
    expect(comparePackVersions('1.0.0', '1.0.1')).toBe(-1);
    expect(comparePackVersions('1.2.0', '1.1.9')).toBe(1);
    expect(comparePackVersions('1.0.0', '1.0.0')).toBe(0);
    expect(comparePackVersions('0.3.0', '0.3.0')).toBe(0);
  });
  it('ignores pre-release suffixes for comparison', () => {
    expect(comparePackVersions('1.0.0-rc1', '1.0.0')).toBe(0);
  });
});

describe('pack-ui-logic: decidePackAction', () => {
  it('install when not installed', () => {
    expect(decidePackAction(null, '1.0.0')).toEqual({ action: 'install', currentVersion: null });
  });
  it('update when incoming is newer', () => {
    expect(decidePackAction('1.0.0', '1.1.0')).toEqual({ action: 'update', currentVersion: '1.0.0' });
  });
  it('reinstall when same version', () => {
    expect(decidePackAction('1.0.0', '1.0.0')).toEqual({ action: 'reinstall', currentVersion: '1.0.0' });
  });
  it('downgrade when incoming is older', () => {
    expect(decidePackAction('2.0.0', '1.0.0')).toEqual({ action: 'downgrade', currentVersion: '2.0.0' });
  });
});
