/**
 * NevoFlux P1 E2E Console Test Script
 *
 * Usage:
 * 1. Open the browser with: npm run start:full
 * 2. Navigate to the test page (file:///path/to/test-page.html)
 * 3. Open Browser Console (Ctrl+Shift+J or Cmd+Shift+J)
 * 4. Copy and paste this entire script
 * 5. Run individual tests or runAllTests()
 *
 * The nevoflux API is available via: browser.nevoflux
 */

// ========== Test Utilities ==========

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const log = (msg, success = null) => {
  const icon = success === true ? '✅' : success === false ? '❌' : '📝';
  console.log(`${icon} ${msg}`);
};

const assert = (condition, msg) => {
  if (!condition) {
    throw new Error(`Assertion failed: ${msg}`);
  }
};

const testResults = { passed: 0, failed: 0, tests: [] };

const runTest = async (name, fn) => {
  try {
    console.group(`🧪 ${name}`);
    await fn();
    testResults.passed++;
    testResults.tests.push({ name, passed: true });
    log('PASSED', true);
    console.groupEnd();
  } catch (e) {
    testResults.failed++;
    testResults.tests.push({ name, passed: false, error: e.message });
    log(`FAILED: ${e.message}`, false);
    console.groupEnd();
  }
};

// ========== API Reference ==========

const api = typeof browser !== 'undefined' ? browser.nevoflux : null;

const checkApi = () => {
  if (!api) {
    console.error('❌ browser.nevoflux is not available!');
    console.log('Make sure:');
    console.log('1. The extension is loaded');
    console.log('2. You are running this in a privileged context');
    return false;
  }
  return true;
};

// ========== Individual Tests ==========

// Version Test
const testVersion = async () => {
  const version = await api.getVersion();
  log(`API Version: ${version}`);
  assert(version, 'Version should be defined');
};

// Tab Management Tests
const testTabManagement = async () => {
  // Create tab
  const createResult = await api.createTab({ url: 'about:blank', active: false });
  log(`createTab result: ${JSON.stringify(createResult)}`);
  assert(createResult.success, 'createTab should succeed');

  const tabId = createResult.tab.id;
  log(`Created tab ID: ${tabId}`);

  // Get tab
  const getResult = await api.getTab(tabId);
  log(`getTab result: ${JSON.stringify(getResult)}`);
  assert(getResult.id === tabId, 'getTab should return correct tab');

  // List tabs
  const listResult = await api.listTabs();
  log(`listTabs count: ${listResult.length}`);
  assert(Array.isArray(listResult), 'listTabs should return array');

  // Query tabs
  const queryResult = await api.queryTabs({ url: 'about:blank' });
  log(`queryTabs result: ${queryResult.length} matching tabs`);

  // Close tab
  const closeResult = await api.closeTab(tabId);
  log(`closeTab result: ${JSON.stringify(closeResult)}`);
  assert(closeResult.success, 'closeTab should succeed');
};

// Click Test
const testClick = async () => {
  // Click the counter button
  const result = await api.click(null, '#click-counter');
  log(`click result: ${JSON.stringify(result)}`);
  assert(result.success, 'click should succeed');

  await delay(100);

  // Verify click was registered (check button text changed)
  const text = await api.getText(null, '#click-counter');
  log(`Button text: ${text}`);
  assert(text.includes('1') || text.includes('2'), 'Click count should increase');
};

// Double Click Test
const testDblClick = async () => {
  const result = await api.dblclick(null, '#dblclick-test');
  log(`dblclick result: ${JSON.stringify(result)}`);
  assert(result.success, 'dblclick should succeed');

  await delay(100);

  const output = await api.getText(null, '#click-output');
  log(`Output: ${output}`);
};

// Type Test
const testType = async () => {
  // Clear first
  await api.clear(null, '#text-input');

  // Type text
  const result = await api.type(null, '#text-input', 'Hello NevoFlux!');
  log(`type result: ${JSON.stringify(result)}`);
  assert(result.success, 'type should succeed');

  await delay(100);

  // Verify
  const value = await api.getValue(null, '#text-input');
  log(`Input value: ${value}`);
  // Note: value may vary depending on implementation
};

// Fill Test
const testFill = async () => {
  const result = await api.fill(null, '#email-input', 'test@example.com');
  log(`fill result: ${JSON.stringify(result)}`);
  assert(result.success, 'fill should succeed');

  await delay(100);

  const value = await api.getValue(null, '#email-input');
  log(`Email value: ${value}`);
};

// Focus and Clear Test
const testFocusAndClear = async () => {
  // Focus
  const focusResult = await api.focus(null, '#password-input');
  log(`focus result: ${JSON.stringify(focusResult)}`);
  assert(focusResult.success, 'focus should succeed');

  // Type something
  await api.type(null, '#password-input', 'secret');

  // Clear
  const clearResult = await api.clear(null, '#password-input');
  log(`clear result: ${JSON.stringify(clearResult)}`);
  assert(clearResult.success, 'clear should succeed');

  const value = await api.getValue(null, '#password-input');
  log(`Password value after clear: "${value}"`);
};

// Keyboard Test
const testKeyboard = async () => {
  // Focus input first
  await api.focus(null, '#keyboard-input');
  await delay(100);

  // Press Enter
  const enterResult = await api.keyPress(null, 'Enter');
  log(`keyPress Enter: ${JSON.stringify(enterResult)}`);

  // Press with modifiers
  const ctrlAResult = await api.keyPress(null, 'a', { modifiers: ['ctrl'] });
  log(`keyPress Ctrl+A: ${JSON.stringify(ctrlAResult)}`);

  // keyDown/keyUp
  const downResult = await api.keyDown(null, 'Shift');
  log(`keyDown Shift: ${JSON.stringify(downResult)}`);

  const upResult = await api.keyUp(null, 'Shift');
  log(`keyUp Shift: ${JSON.stringify(upResult)}`);
};

// Mouse Test
const testMouse = async () => {
  // Move mouse
  const moveResult = await api.mouseMove(null, 400, 300);
  log(`mouseMove: ${JSON.stringify(moveResult)}`);

  // Mouse down
  const downResult = await api.mouseDown(null);
  log(`mouseDown: ${JSON.stringify(downResult)}`);

  // Mouse up
  const upResult = await api.mouseUp(null);
  log(`mouseUp: ${JSON.stringify(upResult)}`);
};

// Wheel (Scroll) Test
const testWheel = async () => {
  // Focus scroll container first
  await api.focus(null, '#scroll-container');
  await delay(100);

  // Scroll down
  const result = await api.wheel(null, { deltaY: 100 });
  log(`wheel: ${JSON.stringify(result)}`);
};

// Visibility Test
const testVisibility = async () => {
  // Check initial state
  const hidden = await api.isVisible(null, '#hidden-element');
  log(`Initial visibility: ${hidden}`);

  // Toggle visibility
  await api.click(null, '#toggle-visibility');
  await delay(100);

  const visible = await api.isVisible(null, '#hidden-element');
  log(`After toggle: ${visible}`);

  // Toggle back
  await api.click(null, '#toggle-visibility');
};

// Exists Test
const testExists = async () => {
  const exists1 = await api.exists(null, '#text-input');
  log(`#text-input exists: ${exists1}`);
  assert(exists1 === true, 'Existing element should return true');

  const exists2 = await api.exists(null, '#nonexistent-element');
  log(`#nonexistent-element exists: ${exists2}`);
  assert(exists2 === false, 'Non-existing element should return false');
};

// Text Extraction Test
const testGetText = async () => {
  const text = await api.getText(null, 'h1');
  log(`H1 text: ${text}`);
  assert(text.includes('NevoFlux'), 'Should contain NevoFlux');
};

// HTML Extraction Test
const testGetHtml = async () => {
  const html = await api.getHtml(null, '#output');
  log(`Output HTML: ${html.substring(0, 100)}...`);
  assert(html.length > 0, 'HTML should not be empty');
};

// LocalStorage Test
const testLocalStorage = async () => {
  // Set
  const setResult = await api.setLocalStorage(null, 'nevoflux_test', 'test_value');
  log(`setLocalStorage: ${JSON.stringify(setResult)}`);

  // Get
  const getResult = await api.getLocalStorage(null, 'nevoflux_test');
  log(`getLocalStorage: ${JSON.stringify(getResult)}`);

  // Remove
  const removeResult = await api.removeLocalStorage(null, 'nevoflux_test');
  log(`removeLocalStorage: ${JSON.stringify(removeResult)}`);

  // Verify removed
  const getAfter = await api.getLocalStorage(null, 'nevoflux_test');
  log(`After remove: ${JSON.stringify(getAfter)}`);
};

// SessionStorage Test
const testSessionStorage = async () => {
  // Set
  const setResult = await api.setSessionStorage(null, 'session_test', { foo: 'bar' });
  log(`setSessionStorage: ${JSON.stringify(setResult)}`);

  // Get
  const getResult = await api.getSessionStorage(null, 'session_test');
  log(`getSessionStorage: ${JSON.stringify(getResult)}`);

  // Clear
  const clearResult = await api.clearSessionStorage(null);
  log(`clearSessionStorage: ${JSON.stringify(clearResult)}`);
};

// Cookie Test
const testCookies = async () => {
  const url = window.location.href;

  // Set cookie
  const setResult = await api.setCookie({
    url: url,
    name: 'nevoflux_test',
    value: 'cookie_value',
  });
  log(`setCookie: ${JSON.stringify(setResult)}`);

  // Get cookies
  const cookies = await api.getCookies();
  log(`getCookies: ${cookies.length} cookies`);

  // Delete cookie
  const deleteResult = await api.deleteCookies({ name: 'nevoflux_test' });
  log(`deleteCookies: ${JSON.stringify(deleteResult)}`);
};

// JavaScript Execution Test
const testEval = async () => {
  // Simple eval
  const result1 = await api.eval(null, '1 + 1');
  log(`eval "1 + 1": ${JSON.stringify(result1)}`);

  // Eval with return value
  const result2 = await api.eval(null, 'document.title');
  log(`eval "document.title": ${JSON.stringify(result2)}`);

  // Eval function call
  const result3 = await api.eval(null, 'window.testFunction("from API")');
  log(`eval testFunction: ${JSON.stringify(result3)}`);
};

// Script Injection Test
const testAddRemoveScript = async () => {
  // Add script
  const addResult = await api.addScript(null, 'window.testGlobalVar = "injected";');
  log(`addScript: ${JSON.stringify(addResult)}`);

  await delay(100);

  // Verify
  const varResult = await api.eval(null, 'window.testGlobalVar');
  log(`testGlobalVar: ${JSON.stringify(varResult)}`);

  // Remove script
  if (addResult.success && addResult.handle) {
    const removeResult = await api.removeScript(null, addResult.handle);
    log(`removeScript: ${JSON.stringify(removeResult)}`);
  }
};

// Network Capture Test
const testNetworkCapture = async () => {
  // Start capture
  const handle = await api.startCapture({ urlPattern: '*' });
  log(`startCapture handle: ${handle}`);

  // Get captures (should be empty initially)
  const captures = await api.getCaptures(handle);
  log(`getCaptures: ${JSON.stringify(captures)}`);

  // Stop capture
  const stopResult = await api.stopCapture(handle);
  log(`stopCapture: ${JSON.stringify(stopResult)}`);
};

// Network Intercept Test
const testNetworkIntercept = async () => {
  // Create intercept
  const handle = await api.intercept({ urlPattern: '*.json' });
  log(`intercept handle: ${handle}`);

  // Remove intercept
  const removeResult = await api.removeIntercept(handle);
  log(`removeIntercept: ${JSON.stringify(removeResult)}`);

  // Clear all
  const clearResult = await api.clearIntercepts();
  log(`clearIntercepts: ${JSON.stringify(clearResult)}`);
};

// Wait For Selector Test
const testWaitForSelector = async () => {
  // Wait for existing element
  const result1 = await api.waitForSelector(null, '#text-input', { timeout: 1000 });
  log(`waitForSelector existing: ${JSON.stringify(result1)}`);

  // Wait for delayed element (click button first)
  await api.click(null, '#load-delayed');
  const result2 = await api.waitForSelector(null, '#delayed-element', { timeout: 5000 });
  log(`waitForSelector delayed: ${JSON.stringify(result2)}`);
};

// Snapshot Test
const testSnapshot = async () => {
  const result = await api.snapshot(null, { compact: true });
  log(
    `snapshot: tree length = ${result.tree?.length || 0}, refs count = ${Object.keys(result.refs || {}).length}`
  );
};

// Privacy Test
const testPrivacy = async () => {
  // Get config
  const config = await api.getPrivacyConfig();
  log(`getPrivacyConfig: ${JSON.stringify(config)}`);

  // Filter sensitive
  const filtered = await api.filterSensitive('Email: test@example.com Phone: 13812345678');
  log(`filterSensitive: ${JSON.stringify(filtered)}`);

  // Set config
  const newConfig = await api.setPrivacyConfig({ enabled: true });
  log(`setPrivacyConfig: ${JSON.stringify(newConfig)}`);
};

// ========== Run All Tests ==========

const runAllTests = async () => {
  if (!checkApi()) return;

  console.clear();
  console.log('🚀 Starting NevoFlux P1 E2E Tests\n');
  console.log('='.repeat(50));

  testResults.passed = 0;
  testResults.failed = 0;
  testResults.tests = [];

  await runTest('Version', testVersion);
  await runTest('Tab Management', testTabManagement);
  await runTest('Click', testClick);
  await runTest('Double Click', testDblClick);
  await runTest('Type', testType);
  await runTest('Fill', testFill);
  await runTest('Focus & Clear', testFocusAndClear);
  await runTest('Keyboard', testKeyboard);
  await runTest('Mouse', testMouse);
  await runTest('Wheel/Scroll', testWheel);
  await runTest('Visibility', testVisibility);
  await runTest('Exists', testExists);
  await runTest('GetText', testGetText);
  await runTest('GetHtml', testGetHtml);
  await runTest('LocalStorage', testLocalStorage);
  await runTest('SessionStorage', testSessionStorage);
  await runTest('Cookies', testCookies);
  await runTest('Eval', testEval);
  await runTest('AddScript/RemoveScript', testAddRemoveScript);
  await runTest('Network Capture', testNetworkCapture);
  await runTest('Network Intercept', testNetworkIntercept);
  await runTest('WaitForSelector', testWaitForSelector);
  await runTest('Snapshot', testSnapshot);
  await runTest('Privacy', testPrivacy);

  console.log('\n' + '='.repeat(50));
  console.log(`📊 Test Results: ${testResults.passed} passed, ${testResults.failed} failed`);
  console.log(
    `Pass Rate: ${((testResults.passed / (testResults.passed + testResults.failed)) * 100).toFixed(1)}%`
  );

  if (testResults.failed > 0) {
    console.log('\n❌ Failed tests:');
    testResults.tests
      .filter((t) => !t.passed)
      .forEach((t) => {
        console.log(`  - ${t.name}: ${t.error}`);
      });
  }

  return testResults;
};

// ========== Quick Test Commands ==========

console.log(`
╔════════════════════════════════════════════════════════════╗
║         NevoFlux P1 E2E Console Test Script                ║
╠════════════════════════════════════════════════════════════╣
║ Available commands:                                         ║
║                                                             ║
║   runAllTests()     - Run all tests                         ║
║   checkApi()        - Verify API is available               ║
║                                                             ║
║ Individual tests:                                           ║
║   testVersion()     testTabManagement()   testClick()       ║
║   testDblClick()    testType()            testFill()        ║
║   testFocusAndClear() testKeyboard()      testMouse()       ║
║   testWheel()       testVisibility()      testExists()      ║
║   testGetText()     testGetHtml()         testLocalStorage()║
║   testSessionStorage() testCookies()      testEval()        ║
║   testAddRemoveScript() testNetworkCapture()                ║
║   testNetworkIntercept() testWaitForSelector()              ║
║   testSnapshot()    testPrivacy()                           ║
║                                                             ║
║ Tip: Run checkApi() first to verify the API is available   ║
╚════════════════════════════════════════════════════════════╝
`);

// Auto-check API availability
checkApi();
