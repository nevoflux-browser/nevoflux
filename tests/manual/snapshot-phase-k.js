/**
 * Manual Integration Tests: Keyword-Driven Snapshot (Phase K)
 *
 * HOW TO RUN:
 * 1. npm run start
 * 2. Navigate to about:debugging#/runtime/this-firefox
 * 3. Find NevoFlux Agent extension → click "Inspect"
 * 4. Paste this entire file into the extension console
 * 5. Navigate to a test page (e.g., github.com, x.com) in a tab
 * 6. Run individual tests: await Test1(), await Test2(), etc.
 *    Or run all: await runAll()
 */

// ── Helpers ──

function log(label, data) {
  console.group(`%c[Phase K Test] ${label}`, 'color: #4fc3f7; font-weight: bold');
  if (typeof data === 'string') {
    console.log(data);
  } else {
    console.dir(data, { depth: 4 });
  }
  console.groupEnd();
}

function pass(name) {
  console.log(
    `%c  PASS  %c ${name}`,
    'background: #4caf50; color: white; padding: 2px 6px; border-radius: 3px',
    ''
  );
}

function fail(name, reason) {
  console.log(
    `%c  FAIL  %c ${name}: ${reason}`,
    'background: #f44336; color: white; padding: 2px 6px; border-radius: 3px',
    ''
  );
}

function info(msg) {
  console.log(
    `%c  INFO  %c ${msg}`,
    'background: #2196f3; color: white; padding: 2px 6px; border-radius: 3px',
    ''
  );
}

// ── Test 1: Backward Compatibility (no keywords) ──

async function Test1() {
  info('Test 1: Backward Compatibility (no keywords)');
  info('Navigate to any page before running this test.');
  try {
    const r = await browser.nevoflux.snapshot(null, {});
    const tree = r.tree;
    const stats = r.stats;

    if (!tree || tree.length === 0) {
      fail('Test 1', 'tree is empty');
      return;
    }

    if (tree.includes('=== KEYWORD MATCHES ===')) {
      fail('Test 1', 'KEYWORD MATCHES section present without keywords');
      return;
    }

    log('Stats', stats);
    log('Tree (first 500 chars)', tree.substring(0, 500));
    pass('Test 1 — No keywords produces normal output, no keyword sections');
  } catch (e) {
    fail('Test 1', e.message);
  }
}

// ── Test 2: lang: Field in Output ──

async function Test2() {
  info('Test 2: lang: Field in Output');
  info('Navigate to an English page (e.g., google.com) before running.');
  try {
    const r = await browser.nevoflux.snapshot(null, {});
    const lang = r.viewportInfo?.lang;
    const langLine = r.tree.match(/^lang:.*/m);

    log('viewportInfo.lang', lang);
    log('lang line in tree', langLine ? langLine[0] : '(none)');

    if (lang === undefined) {
      fail('Test 2', 'viewportInfo.lang is missing');
      return;
    }

    if (lang && !langLine) {
      fail('Test 2', `viewportInfo.lang="${lang}" but no lang: line in tree`);
      return;
    }

    pass('Test 2 — lang field present in viewportInfo and compact tree');
    info('Try navigating to baidu.com and running Test2() again for zh/zh-CN');
  } catch (e) {
    fail('Test 2', e.message);
  }
}

// ── Test 3: Keyword Search on English Page ──

async function Test3() {
  info('Test 3: Keyword Search');
  info('Navigate to github.com or x.com before running.');
  try {
    const r = await browser.nevoflux.snapshot(null, { keywords: ['Search', 'Home'] });
    const tree = r.tree;

    log('Full tree output', tree);

    if (!tree.includes('=== KEYWORD MATCHES ===')) {
      fail('Test 3', 'Missing KEYWORD MATCHES section');
      return;
    }

    if (!tree.includes('=== OTHER INTERACTABLES ===')) {
      fail('Test 3', 'Missing OTHER INTERACTABLES section');
      return;
    }

    const keywordAnnotation = tree.match(/\(keywords:.*?\)/);
    if (!keywordAnnotation) {
      fail('Test 3', 'No keyword annotations found on elements');
      return;
    }

    log('Sample keyword annotation', keywordAnnotation[0]);
    pass('Test 3 — Keyword sections and annotations present');
  } catch (e) {
    fail('Test 3', e.message);
  }
}

// ── Test 4: Icon-Only Button Fallback (aria-label) ──

async function Test4() {
  info('Test 4: Icon-Only Button Fallback (aria-label/title)');
  info('Navigate to x.com (many icon buttons) before running.');
  try {
    const r = await browser.nevoflux.snapshot(null, { keywords: ['Post', 'Search'] });
    const tree = r.tree;

    log('Tree output', tree);

    if (tree.includes('=== KEYWORD MATCHES ===')) {
      // Check for any matched elements
      const lines = tree.split('\n');
      const kwSection = [];
      let inKwSection = false;
      for (const line of lines) {
        if (line.includes('=== KEYWORD MATCHES ===')) {
          inKwSection = true;
          continue;
        }
        if (line.includes('===')) {
          inKwSection = false;
          continue;
        }
        if (inKwSection && line.trim()) kwSection.push(line);
      }
      log(`Keyword matched elements (${kwSection.length})`, kwSection.join('\n'));

      if (kwSection.length > 0) {
        pass('Test 4 — Icon buttons found via aria-label/title fallback');
      } else {
        fail('Test 4', 'KEYWORD MATCHES section exists but is empty');
      }
    } else {
      info('No keyword matches found. The page may not have matching aria-labels.');
      info('Try different keywords or a different page.');
    }
  } catch (e) {
    fail('Test 4', e.message);
  }
}

// ── Test 5: Dedup Behavior ──

async function Test5() {
  info('Test 5: Dedup Behavior');
  info('Navigate to a page with a Home link/button before running.');
  try {
    const r = await browser.nevoflux.snapshot(null, { keywords: ['Home', 'Search', 'Home'] });
    const tree = r.tree;

    // Count unique elements in keyword section
    const lines = tree.split('\n');
    const kwElements = [];
    let inKwSection = false;
    for (const line of lines) {
      if (line.includes('=== KEYWORD MATCHES ===')) {
        inKwSection = true;
        continue;
      }
      if (line.includes('===')) {
        inKwSection = false;
        continue;
      }
      if (inKwSection && line.trim() && line.match(/^\[/)) kwElements.push(line);
    }

    log(`Keyword elements (${kwElements.length})`, kwElements.join('\n'));

    // Check for merged keywords
    const mergedLine = kwElements.find((l) => l.includes('"Home"') && l.includes('"Search"'));
    if (mergedLine) {
      log('Merged keywords', mergedLine);
      pass('Test 5 — Keywords merged on same element');
    } else {
      info('No element matched both "Home" and "Search". Check that elements appear only once.');
      // At least verify no duplicates
      const ids = kwElements
        .map((l) => {
          const m = l.match(/^\[(\d+)\]/);
          return m ? m[1] : null;
        })
        .filter(Boolean);
      const uniqueIds = new Set(ids);
      if (ids.length === uniqueIds.size) {
        pass('Test 5 — No duplicate elements in keyword section');
      } else {
        fail('Test 5', `Found ${ids.length - uniqueIds.size} duplicate element(s)`);
      }
    }
  } catch (e) {
    fail('Test 5', e.message);
  }
}

// ── Test 6: Keyword-Matched Elements Survive Truncation ──

async function Test6() {
  info('Test 6: Keyword elements survive truncation (maxElements=5)');
  info('Navigate to a content-rich page before running.');
  try {
    const r = await browser.nevoflux.snapshot(null, { keywords: ['Search'], maxElements: 5 });
    const tree = r.tree;
    const stats = r.stats;

    log('Stats', stats);
    log('Tree', tree);

    if (tree.includes('=== KEYWORD MATCHES ===')) {
      // Verify keyword matches are present
      const kwLines = tree.split('\n').filter((l) => l.includes('keywords:'));
      log(`Keyword-annotated elements: ${kwLines.length}`, kwLines.join('\n'));

      if (stats && typeof stats.truncated === 'number') {
        log('Truncated count', stats.truncated);
        pass('Test 6 — Keyword elements preserved with truncation active');
      } else {
        info('stats.truncated not found — page may have <= 5 elements total');
        pass('Test 6 — Keyword elements present (page may be small)');
      }
    } else {
      info('No keyword matches found for "Search". Try a different keyword.');
    }
  } catch (e) {
    fail('Test 6', e.message);
  }
}

// ── Test 7: No Matches ──

async function Test7() {
  info('Test 7: No Matches (nonsense keyword)');
  try {
    const r = await browser.nevoflux.snapshot(null, { keywords: ['xyzzy_nonexistent_12345'] });
    const tree = r.tree;

    if (tree.includes('=== KEYWORD MATCHES ===')) {
      fail('Test 7', 'KEYWORD MATCHES section present for nonsense keyword');
      return;
    }

    log('Tree (first 500 chars)', tree.substring(0, 500));
    pass('Test 7 — No keyword section for unmatched keyword');
  } catch (e) {
    fail('Test 7', e.message);
  }
}

// ── Run All ──

async function runAll() {
  console.log(
    '%c\n=== Phase K Integration Tests ===\n',
    'color: #ff9800; font-size: 16px; font-weight: bold'
  );
  info('Make sure you have navigated to a content-rich page (github.com, x.com, etc.)');
  console.log('');

  await Test1();
  console.log('');
  await Test2();
  console.log('');
  await Test3();
  console.log('');
  await Test4();
  console.log('');
  await Test5();
  console.log('');
  await Test6();
  console.log('');
  await Test7();

  console.log(
    '%c\n=== All Tests Complete ===\n',
    'color: #ff9800; font-size: 16px; font-weight: bold'
  );
}

// Ready message
console.log(
  '%c Phase K Test Suite Loaded ',
  'background: #673ab7; color: white; padding: 4px 12px; border-radius: 4px; font-size: 14px'
);
console.log('Commands:');
console.log('  await Test1()  — Backward compatibility (no keywords)');
console.log('  await Test2()  — lang: field');
console.log('  await Test3()  — Keyword search');
console.log('  await Test4()  — Icon-only buttons (aria-label fallback)');
console.log('  await Test5()  — Dedup behavior');
console.log('  await Test6()  — Truncation protection');
console.log('  await Test7()  — No matches (nonsense keyword)');
console.log('  await runAll() — Run all tests');
