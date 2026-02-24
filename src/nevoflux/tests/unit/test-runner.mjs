/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Simple test runner for NevoFlux P1 unit tests
 * Runs tests without requiring Jest or other external dependencies
 */

const colors = {
  reset: '\x1b[0m',
  green: '\x1b[32m',
  red: '\x1b[31m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

class TestRunner {
  constructor() {
    this.suites = [];
    this.currentSuite = null;
    this.results = {
      total: 0,
      passed: 0,
      failed: 0,
      skipped: 0,
    };
  }

  describe(name, fn) {
    const suite = {
      name,
      tests: [],
      beforeEach: null,
      afterEach: null,
    };
    this.suites.push(suite);
    this.currentSuite = suite;
    fn();
    this.currentSuite = null;
  }

  it(name, fn) {
    if (this.currentSuite) {
      this.currentSuite.tests.push({ name, fn, skip: false });
    }
  }

  skip(name, fn) {
    if (this.currentSuite) {
      this.currentSuite.tests.push({ name, fn, skip: true });
    }
  }

  beforeEach(fn) {
    if (this.currentSuite) {
      this.currentSuite.beforeEach = fn;
    }
  }

  afterEach(fn) {
    if (this.currentSuite) {
      this.currentSuite.afterEach = fn;
    }
  }

  async run() {
    console.log(`\n${colors.cyan}Running NevoFlux P1 Unit Tests${colors.reset}\n`);
    console.log('='.repeat(60));

    for (const suite of this.suites) {
      console.log(`\n${colors.yellow}${suite.name}${colors.reset}`);

      for (const test of suite.tests) {
        this.results.total++;

        if (test.skip) {
          this.results.skipped++;
          console.log(`  ${colors.dim}○ SKIP: ${test.name}${colors.reset}`);
          continue;
        }

        try {
          if (suite.beforeEach) await suite.beforeEach();
          await test.fn();
          if (suite.afterEach) await suite.afterEach();

          this.results.passed++;
          console.log(`  ${colors.green}✓ PASS: ${test.name}${colors.reset}`);
        } catch (error) {
          this.results.failed++;
          console.log(`  ${colors.red}✗ FAIL: ${test.name}${colors.reset}`);
          console.log(`    ${colors.dim}${error.message}${colors.reset}`);
          if (error.stack) {
            const stackLines = error.stack.split('\n').slice(1, 3);
            stackLines.forEach((line) => {
              console.log(`    ${colors.dim}${line.trim()}${colors.reset}`);
            });
          }
        }
      }
    }

    this.printSummary();
    return this.results;
  }

  printSummary() {
    console.log('\n' + '='.repeat(60));
    console.log(`${colors.cyan}Test Summary${colors.reset}`);
    console.log('='.repeat(60));

    const passRate =
      this.results.total > 0 ? ((this.results.passed / this.results.total) * 100).toFixed(1) : 0;

    console.log(`Total:   ${this.results.total}`);
    console.log(`${colors.green}Passed:  ${this.results.passed}${colors.reset}`);
    console.log(`${colors.red}Failed:  ${this.results.failed}${colors.reset}`);
    console.log(`${colors.dim}Skipped: ${this.results.skipped}${colors.reset}`);
    console.log(`Pass Rate: ${passRate}%`);

    if (this.results.failed === 0) {
      console.log(`\n${colors.green}All tests passed!${colors.reset}`);
    } else {
      console.log(`\n${colors.red}${this.results.failed} test(s) failed${colors.reset}`);
    }
  }
}

// Assertion helpers
export function expect(actual) {
  return {
    toBe(expected) {
      if (actual !== expected) {
        throw new Error(`Expected ${JSON.stringify(expected)}, but got ${JSON.stringify(actual)}`);
      }
    },
    toEqual(expected) {
      const actualStr = JSON.stringify(actual);
      const expectedStr = JSON.stringify(expected);
      if (actualStr !== expectedStr) {
        throw new Error(`Expected ${expectedStr}, but got ${actualStr}`);
      }
    },
    toBeTruthy() {
      if (!actual) {
        throw new Error(`Expected truthy value, but got ${JSON.stringify(actual)}`);
      }
    },
    toBeFalsy() {
      if (actual) {
        throw new Error(`Expected falsy value, but got ${JSON.stringify(actual)}`);
      }
    },
    toBeNull() {
      if (actual !== null) {
        throw new Error(`Expected null, but got ${JSON.stringify(actual)}`);
      }
    },
    toBeUndefined() {
      if (actual !== undefined) {
        throw new Error(`Expected undefined, but got ${JSON.stringify(actual)}`);
      }
    },
    toBeDefined() {
      if (actual === undefined) {
        throw new Error(`Expected defined value, but got undefined`);
      }
    },
    toContain(item) {
      if (Array.isArray(actual)) {
        if (!actual.includes(item)) {
          throw new Error(`Expected array to contain ${JSON.stringify(item)}`);
        }
      } else if (typeof actual === 'string') {
        if (!actual.includes(item)) {
          throw new Error(`Expected string to contain "${item}"`);
        }
      } else {
        throw new Error(`toContain can only be used with arrays or strings`);
      }
    },
    toHaveProperty(key, value) {
      if (!(key in actual)) {
        throw new Error(`Expected object to have property "${key}"`);
      }
      if (value !== undefined && actual[key] !== value) {
        throw new Error(
          `Expected property "${key}" to be ${JSON.stringify(value)}, but got ${JSON.stringify(actual[key])}`
        );
      }
    },
    toBeGreaterThan(num) {
      if (actual <= num) {
        throw new Error(`Expected ${actual} to be greater than ${num}`);
      }
    },
    toBeGreaterThanOrEqual(num) {
      if (actual < num) {
        throw new Error(`Expected ${actual} to be greater than or equal to ${num}`);
      }
    },
    toBeLessThan(num) {
      if (actual >= num) {
        throw new Error(`Expected ${actual} to be less than ${num}`);
      }
    },
    toMatch(regex) {
      if (!regex.test(actual)) {
        throw new Error(`Expected "${actual}" to match ${regex}`);
      }
    },
    toThrow(expectedError) {
      let thrown = false;
      let error = null;
      try {
        actual();
      } catch (e) {
        thrown = true;
        error = e;
      }
      if (!thrown) {
        throw new Error(`Expected function to throw`);
      }
      if (expectedError && error.message !== expectedError) {
        throw new Error(`Expected error message "${expectedError}", but got "${error.message}"`);
      }
    },
    not: {
      toBe(expected) {
        if (actual === expected) {
          throw new Error(
            `Expected ${JSON.stringify(actual)} not to be ${JSON.stringify(expected)}`
          );
        }
      },
      toEqual(expected) {
        const actualStr = JSON.stringify(actual);
        const expectedStr = JSON.stringify(expected);
        if (actualStr === expectedStr) {
          throw new Error(`Expected not to equal ${expectedStr}`);
        }
      },
      toBeNull() {
        if (actual === null) {
          throw new Error(`Expected not to be null`);
        }
      },
      toBeUndefined() {
        if (actual === undefined) {
          throw new Error(`Expected not to be undefined`);
        }
      },
      toContain(item) {
        if (Array.isArray(actual) && actual.includes(item)) {
          throw new Error(`Expected array not to contain ${JSON.stringify(item)}`);
        }
        if (typeof actual === 'string' && actual.includes(item)) {
          throw new Error(`Expected string not to contain "${item}"`);
        }
      },
    },
  };
}

// Export singleton instance
export const runner = new TestRunner();
export const describe = runner.describe.bind(runner);
export const it = runner.it.bind(runner);
export const skip = runner.skip.bind(runner);
export const beforeEach = runner.beforeEach.bind(runner);
export const afterEach = runner.afterEach.bind(runner);
