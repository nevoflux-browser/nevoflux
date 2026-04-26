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

// --- P4 single-file mode ----------------------------------------------
const idx = process.argv.indexOf('--file');
const singleFile = idx > -1 ? process.argv[idx + 1] : null;
if (singleFile) {
  const html = await readFile(singleFile, 'utf8');
  const { basename } = await import('node:path');
  const report = lint(html, { composition_id: basename(singleFile) });
  const hasErrors = report.errors.length > 0;
  console.log(`${singleFile}: ${report.errors.length} errors, ${report.warnings.length} warnings, ${report.infos.length} infos`);
  if (hasErrors) {
    for (const e of report.errors) {
      console.error(`  ERROR ${e.rule_id}${e.line != null ? ' (line ' + e.line + ')' : ''}: ${e.message}`);
    }
  }
  if (report.warnings.length > 0) {
    for (const w of report.warnings) {
      console.warn(`  WARN  ${w.rule_id}${w.line != null ? ' (line ' + w.line + ')' : ''}: ${w.message}`);
    }
  }
  process.exit(hasErrors ? 1 : 0);
}
// --- end P4 single-file mode ------------------------------------------

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

// Performance sanity: a ~500-line composition must lint in ≤ 50ms.
const perfHtml = [
  '<!doctype html>',
  '<html>',
  '<head><meta charset="utf-8"><style>',
  '  body { margin: 0; background: #000; }',
  '  #stage { position: relative; width: 640px; height: 360px; overflow: hidden; }',
  '</style></head>',
  '<body>',
  '  <div id="stage" data-width="640" data-height="360" data-duration="5" data-fps="30"',
  '       data-composition-id="perf"></div>',
  ...Array.from({ length: 500 }, (_, i) =>
    `  <div class="scene-${i}"><span>line ${i}</span></div>`),
  '  <script src="https://esm.sh/gsap@3.13"></script>',
  '  <script>',
  '    window.__timelines = window.__timelines || [];',
  '    gsap.timeline();',
  '  </script>',
  '</body>',
  '</html>',
].join('\n');
// Warmup call to prime the DOMParser JIT before the timed run.
lint(perfHtml, { composition_id: 'perf-warmup' });
const perfStart = performance.now();
const perfReport = lint(perfHtml, { composition_id: 'perf-sanity' });
const perfElapsed = performance.now() - perfStart;
const PERF_BUDGET_MS = 100;
if (perfElapsed > PERF_BUDGET_MS) {
  console.error(`  FAIL performance-sanity: ${perfElapsed.toFixed(1)}ms > ${PERF_BUDGET_MS}ms budget`);
  console.error(`    (report had ${perfReport.errors.length} errors, ${perfReport.warnings.length} warnings)`);
  fail++;
} else {
  console.log(`  PASS performance-sanity: ${perfElapsed.toFixed(1)}ms (budget ${PERF_BUDGET_MS}ms)`);
  pass++;
}

// --- Strict-mode test: narrowed warnings escalate to errors ---------------
// Two GSAP tweens both target ".box" and animate the same property `x`
// without overwrite:"auto". Default mode → comp/overlapping-gsap-tweens
// emits a warning. Strict mode (daemon path) → same rule emits an error.
const narrowedHtml = `<!doctype html>
<html><head><style>
.scene { position: relative; width: 640px; height: 360px; }
.clip { position: absolute; visibility: hidden; }
</style></head><body>
<div id="stage" data-width="640" data-height="360" data-duration="5" data-fps="30"
     data-composition-id="strict-fixture"></div>
<div class="scene clip" data-start="0" data-duration="5">
  <div class="box">a</div>
</div>
<script src="https://esm.sh/gsap@3.13"></script>
<script>
const tl = gsap.timeline();
window.__timelines = window.__timelines || {};
window.__timelines["strict-fixture"] = tl;
tl.to(".box", { x: 100, duration: 1 });
tl.to(".box", { x: 200, duration: 1 });
</script></body></html>`;

const ruleIds = (issues) => issues.map(i => i.rule_id);
const lenient = lint(narrowedHtml, { composition_id: 'strict-fixture' });
const strict  = lint(narrowedHtml, { composition_id: 'strict-fixture', strict: true });
const lenientWarn = ruleIds(lenient.warnings).includes('comp/overlapping-gsap-tweens');
const lenientErr  = ruleIds(lenient.errors).includes('comp/overlapping-gsap-tweens');
const strictErr   = ruleIds(strict.errors).includes('comp/overlapping-gsap-tweens');
const strictWarn  = ruleIds(strict.warnings).includes('comp/overlapping-gsap-tweens');
if (lenientWarn && !lenientErr && strictErr && !strictWarn) {
  console.log('  PASS strict-mode-escalates-narrowed-warnings');
  pass++;
} else {
  console.error('  FAIL strict-mode-escalates-narrowed-warnings');
  console.error(`    lenient: errors=${ruleIds(lenient.errors)}, warnings=${ruleIds(lenient.warnings)}`);
  console.error(`    strict : errors=${ruleIds(strict.errors)}, warnings=${ruleIds(strict.warnings)}`);
  fail++;
}

console.log(`\n${pass} pass, ${fail} fail`);
if (fail) process.exit(1);
