// composition-linter/tests/run.mjs — pure Node ESM fixture runner.
import { readdir, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { JSDOM } from 'jsdom';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Install DOMParser + Element globals for the linter.
const dom = new JSDOM('<!doctype html><html><body></body></html>');
globalThis.DOMParser = dom.window.DOMParser;
globalThis.Document  = dom.window.Document;
globalThis.Element   = dom.window.Element;
globalThis.HTMLElement = dom.window.HTMLElement;
globalThis.Node      = dom.window.Node;

const { lint, LINTER_VERSION } = await import('../index.js');

console.log(`composition-linter tests — version ${LINTER_VERSION}`);

const fixturesDir = join(__dirname, 'fixtures');
const entries = (await readdir(fixturesDir)).filter(f => f.endsWith('.html'));

let pass = 0, fail = 0;
for (const file of entries) {
  const html = await readFile(join(fixturesDir, file), 'utf8');
  const expectedPath = join(fixturesDir, file.replace(/\.html$/, '.expected.json'));
  let expected;
  try { expected = JSON.parse(await readFile(expectedPath, 'utf8')); }
  catch { console.error(`NO expected JSON: ${file}`); fail++; continue; }

  const report = lint(html, { composition_id: file });
  const got = {
    errors:   report.errors.map(i => i.rule_id).sort(),
    warnings: report.warnings.map(i => i.rule_id).sort(),
    infos:    report.infos.map(i => i.rule_id).sort(),
  };
  const exp = {
    errors:   (expected.errors   || []).slice().sort(),
    warnings: (expected.warnings || []).slice().sort(),
    infos:    (expected.infos    || []).slice().sort(),
  };

  if (JSON.stringify(got) === JSON.stringify(exp)) {
    console.log(`  PASS ${file}`);
    pass++;
  } else {
    console.error(`  FAIL ${file}`);
    console.error('    expected:', JSON.stringify(exp));
    console.error('    got     :', JSON.stringify(got));
    fail++;
  }
}

console.log(`\n${pass} pass, ${fail} fail`);
if (fail) process.exit(1);
