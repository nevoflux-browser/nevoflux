#!/usr/bin/env node
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Main test entry point for NevoFlux P1 unit tests
 * Runs all tests and reports coverage/pass rate
 */

import { runner } from './test-runner.mjs';

// Import all test files to register their test suites
import './nevoflux-child.test.mjs';
import './ext-nevoflux.test.mjs';
import './pack-ui-logic.test.mjs';
import './recorder-logic.test.mjs';

// Run all tests
const results = await runner.run();

// Exit with appropriate code
if (results.failed === 0) {
  console.log('\n\x1b[32mAll tests passed successfully!\x1b[0m\n');
  process.exit(0);
} else {
  const passRate = ((results.passed / results.total) * 100).toFixed(1);
  if (parseFloat(passRate) >= 80) {
    console.log(`\n\x1b[33mPass rate ${passRate}% meets 80% threshold\x1b[0m\n`);
    process.exit(0);
  } else {
    console.log(`\n\x1b[31mPass rate ${passRate}% below 80% threshold\x1b[0m\n`);
    process.exit(1);
  }
}
