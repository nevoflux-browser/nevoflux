# NevoFlux P1 E2E Test Checklist

## Prerequisites

### Setup

- [ ] Browser built: `npm run build`
- [ ] Extension loaded: `npm run reload-ext`
- [ ] Browser started: `npm run start`
- [ ] Test page opened: `file://<project-root>/src/nevoflux/tests/e2e/test-page.html`
- [ ] Browser Console opened: `Ctrl+Shift+J` (or `Cmd+Shift+J` on Mac)

### API Verification

- [ ] Run `browser.nevoflux` in console - should return API object
- [ ] Run `browser.nevoflux.getVersion()` - should return "1.0.0"

---

## Test Categories

### 1. Version & Basic API

| Test        | Command                         | Expected        | Status |
| ----------- | ------------------------------- | --------------- | ------ |
| Get version | `browser.nevoflux.getVersion()` | Returns "1.0.0" | [ ]    |

### 2. Tab Management

| Test          | Command                                            | Expected                               | Status |
| ------------- | -------------------------------------------------- | -------------------------------------- | ------ |
| Create tab    | `browser.nevoflux.createTab({url: 'about:blank'})` | Returns {success: true, tab: {...}}    | [ ]    |
| Get tab       | `browser.nevoflux.getTab(tabId)`                   | Returns tab info object                | [ ]    |
| List tabs     | `browser.nevoflux.listTabs()`                      | Returns array of tabs                  | [ ]    |
| Query tabs    | `browser.nevoflux.queryTabs({url: '*'})`           | Returns filtered tabs                  | [ ]    |
| Activate tab  | `browser.nevoflux.activateTab(tabId)`              | Tab becomes active                     | [ ]    |
| Close tab     | `browser.nevoflux.closeTab(tabId)`                 | Returns {success: true}                | [ ]    |
| Create window | `browser.nevoflux.createWindow()`                  | Returns {success: true, windowId: ...} | [ ]    |
| Close window  | `browser.nevoflux.closeWindow(windowId)`           | Returns {success: true}                | [ ]    |

### 3. Click & Interaction

| Test          | Command                                             | Expected                            | Status |
| ------------- | --------------------------------------------------- | ----------------------------------- | ------ |
| Click button  | `browser.nevoflux.click(null, '#click-counter')`    | Counter increments, {success: true} | [ ]    |
| Double click  | `browser.nevoflux.dblclick(null, '#dblclick-test')` | "Double click detected!" in output  | [ ]    |
| Focus element | `browser.nevoflux.focus(null, '#text-input')`       | Element receives focus              | [ ]    |
| Clear input   | `browser.nevoflux.clear(null, '#text-input')`       | Input value cleared                 | [ ]    |

### 4. Type & Fill

| Test      | Command                                                        | Expected              | Status |
| --------- | -------------------------------------------------------------- | --------------------- | ------ |
| Type text | `browser.nevoflux.type(null, '#text-input', 'Hello')`          | Text appears in input | [ ]    |
| Fill text | `browser.nevoflux.fill(null, '#email-input', 'test@test.com')` | Email value set       | [ ]    |
| Get value | `browser.nevoflux.getValue(null, '#text-input')`               | Returns input value   | [ ]    |

### 5. Keyboard Control

| Test              | Command                                                       | Expected          | Status |
| ----------------- | ------------------------------------------------------------- | ----------------- | ------ |
| Key press         | `browser.nevoflux.keyPress(null, 'Enter')`                    | {success: true}   | [ ]    |
| Key with modifier | `browser.nevoflux.keyPress(null, 'a', {modifiers: ['ctrl']})` | Ctrl+A registered | [ ]    |
| Key down          | `browser.nevoflux.keyDown(null, 'Shift')`                     | {success: true}   | [ ]    |
| Key up            | `browser.nevoflux.keyUp(null, 'Shift')`                       | {success: true}   | [ ]    |

### 6. Mouse Control

| Test         | Command                                                       | Expected        | Status |
| ------------ | ------------------------------------------------------------- | --------------- | ------ |
| Mouse move   | `browser.nevoflux.mouseMove(null, 400, 300)`                  | {success: true} | [ ]    |
| Mouse down   | `browser.nevoflux.mouseDown(null)`                            | {success: true} | [ ]    |
| Mouse up     | `browser.nevoflux.mouseUp(null)`                              | {success: true} | [ ]    |
| Wheel scroll | `browser.nevoflux.wheel(null, {deltaY: 100})`                 | {success: true} | [ ]    |
| Drag         | `browser.nevoflux.drag(null, '#drag-source', '#drop-target')` | Element dragged | [ ]    |

### 7. State Checking

| Test           | Command                                               | Expected | Status |
| -------------- | ----------------------------------------------------- | -------- | ------ |
| Exists (true)  | `browser.nevoflux.exists(null, '#text-input')`        | true     | [ ]    |
| Exists (false) | `browser.nevoflux.exists(null, '#nonexistent')`       | false    | [ ]    |
| Is visible     | `browser.nevoflux.isVisible(null, '#text-input')`     | true     | [ ]    |
| Is hidden      | `browser.nevoflux.isVisible(null, '#hidden-element')` | false    | [ ]    |

### 8. Data Extraction

| Test      | Command                                          | Expected                            | Status |
| --------- | ------------------------------------------------ | ----------------------------------- | ------ |
| Get text  | `browser.nevoflux.getText(null, 'h1')`           | Returns "NevoFlux P1 E2E Test Page" | [ ]    |
| Get HTML  | `browser.nevoflux.getHtml(null, '#output')`      | Returns inner HTML                  | [ ]    |
| Get value | `browser.nevoflux.getValue(null, '#text-input')` | Returns input value                 | [ ]    |
| Snapshot  | `browser.nevoflux.snapshot(null, {})`            | Returns {tree, refs}                | [ ]    |

### 9. Cookie Management

| Test               | Command                                                                        | Expected                 | Status |
| ------------------ | ------------------------------------------------------------------------------ | ------------------------ | ------ |
| Set cookie         | `browser.nevoflux.setCookie({url: location.href, name: 'test', value: 'val'})` | {success: true}          | [ ]    |
| Get cookies        | `browser.nevoflux.getCookies()`                                                | Returns array of cookies | [ ]    |
| Get cookie by name | `browser.nevoflux.getCookies({name: 'test'})`                                  | Returns matching cookies | [ ]    |
| Delete cookies     | `browser.nevoflux.deleteCookies({name: 'test'})`                               | {success: true}          | [ ]    |
| Clear cookies      | `browser.nevoflux.clearCookies()`                                              | {success: true}          | [ ]    |

### 10. LocalStorage

| Test              | Command                                                  | Expected                         | Status |
| ----------------- | -------------------------------------------------------- | -------------------------------- | ------ |
| Set local storage | `browser.nevoflux.setLocalStorage(null, 'key', 'value')` | {success: true}                  | [ ]    |
| Get local storage | `browser.nevoflux.getLocalStorage(null, 'key')`          | Returns {success, data: 'value'} | [ ]    |
| Get all storage   | `browser.nevoflux.getLocalStorage(null)`                 | Returns all items                | [ ]    |
| Remove storage    | `browser.nevoflux.removeLocalStorage(null, 'key')`       | {success: true}                  | [ ]    |
| Clear storage     | `browser.nevoflux.clearLocalStorage(null)`               | {success: true}                  | [ ]    |

### 11. SessionStorage

| Test                   | Command                                                         | Expected                | Status |
| ---------------------- | --------------------------------------------------------------- | ----------------------- | ------ |
| Set session storage    | `browser.nevoflux.setSessionStorage(null, 'key', {foo: 'bar'})` | {success: true}         | [ ]    |
| Get session storage    | `browser.nevoflux.getSessionStorage(null, 'key')`               | Returns {success, data} | [ ]    |
| Remove session storage | `browser.nevoflux.removeSessionStorage(null, 'key')`            | {success: true}         | [ ]    |
| Clear session storage  | `browser.nevoflux.clearSessionStorage(null)`                    | {success: true}         | [ ]    |

### 12. Network

| Test             | Command                                         | Expected                  | Status |
| ---------------- | ----------------------------------------------- | ------------------------- | ------ |
| Start capture    | `browser.nevoflux.startCapture({})`             | Returns handle string     | [ ]    |
| Get captures     | `browser.nevoflux.getCaptures(handle)`          | Returns array of requests | [ ]    |
| Stop capture     | `browser.nevoflux.stopCapture(handle)`          | Returns captured requests | [ ]    |
| Intercept        | `browser.nevoflux.intercept({urlPattern: '*'})` | Returns handle            | [ ]    |
| Remove intercept | `browser.nevoflux.removeIntercept(handle)`      | {success: true}           | [ ]    |
| Clear intercepts | `browser.nevoflux.clearIntercepts()`            | {success: true}           | [ ]    |

### 13. JavaScript Execution

| Test            | Command                                                  | Expected                     | Status |
| --------------- | -------------------------------------------------------- | ---------------------------- | ------ |
| Eval simple     | `browser.nevoflux.eval(null, '1 + 1')`                   | {success: true, value: 2}    | [ ]    |
| Eval document   | `browser.nevoflux.eval(null, 'document.title')`          | Returns page title           | [ ]    |
| Eval with error | `browser.nevoflux.eval(null, 'throw new Error("test")')` | {success: false, error: ...} | [ ]    |
| Add script      | `browser.nevoflux.addScript(null, 'window.foo = 1')`     | {success: true, handle: ...} | [ ]    |
| Remove script   | `browser.nevoflux.removeScript(null, handle)`            | {success: true}              | [ ]    |

### 14. Wait Functions

| Test              | Command                                                                                 | Expected         | Status |
| ----------------- | --------------------------------------------------------------------------------------- | ---------------- | ------ |
| Wait for existing | `browser.nevoflux.waitForSelector(null, '#text-input', {timeout: 1000})`                | {success: true}  | [ ]    |
| Wait timeout      | `browser.nevoflux.waitForSelector(null, '#nonexistent', {timeout: 1000})`               | {success: false} | [ ]    |
| Wait for dynamic  | Click "Load Delayed", then `waitForSelector(null, '#delayed-element', {timeout: 5000})` | {success: true}  | [ ]    |

### 15. Privacy API

| Test               | Command                                              | Expected               | Status |
| ------------------ | ---------------------------------------------------- | ---------------------- | ------ |
| Get privacy config | `browser.nevoflux.getPrivacyConfig()`                | Returns config object  | [ ]    |
| Set privacy config | `browser.nevoflux.setPrivacyConfig({enabled: true})` | Returns updated config | [ ]    |
| Filter sensitive   | `browser.nevoflux.filterSensitive('test@test.com')`  | Returns filtered text  | [ ]    |

---

## Automated Test Script

For faster testing, paste the console test script and run:

```javascript
// Load the script from:
// <project-root>/src/nevoflux/tests/e2e/console-test-script.js

// Then run:
runAllTests();
```

---

## Test Results Summary

| Category             | Tests  | Passed | Failed |
| -------------------- | ------ | ------ | ------ |
| Version & Basic API  | 1      |        |        |
| Tab Management       | 8      |        |        |
| Click & Interaction  | 4      |        |        |
| Type & Fill          | 3      |        |        |
| Keyboard Control     | 4      |        |        |
| Mouse Control        | 6      |        |        |
| State Checking       | 4      |        |        |
| Data Extraction      | 4      |        |        |
| Cookie Management    | 5      |        |        |
| LocalStorage         | 5      |        |        |
| SessionStorage       | 4      |        |        |
| Network              | 6      |        |        |
| JavaScript Execution | 5      |        |        |
| Wait Functions       | 3      |        |        |
| Privacy API          | 3      |        |        |
| **Total**            | **65** |        |        |

---

## Issues Found

| Issue | Category | Severity | Notes |
| ----- | -------- | -------- | ----- |
|       |          |          |       |

---

## Notes

- Test page path: `file://<project-root>/src/nevoflux/tests/e2e/test-page.html`
- All tests use `null` for tabId to target the active tab
- Some tests may require visual verification
- Network tests may need actual network requests to fully validate

---

## 20. Browser Input Actor Primitives (PR #1)

Open each fixture HTML page from `fixtures/editors/` in the browser
and verify the commands below via browser console (Ctrl+Shift+J).

### 20.1 Standard Input (fixtures/editors/standard-input.html)

| Test | Command | Expected | Status |
|---|---|---|---|
| probe input | `await browser.nevoflux.probe(null, '#tgt-input')` | `result.tag === 'input'`, `has_value_property === true`, `is_content_editable === false` | [ ] |
| fill input | `await browser.nevoflux.fill(null, '#tgt-input', 'Hello')` | `success: true`; input value becomes "Hello" | [ ] |
| type input | `await browser.nevoflux.type(null, '#tgt-input', ' World')` | Final value "Hello World" (no "undefined" prefix) | [ ] |
| fill textarea | `await browser.nevoflux.fill(null, '#tgt-textarea', 'Multi\nline')` | Textarea shows the multiline text | [ ] |

### 20.2 Simple ContentEditable (simple-contenteditable.html)

| Test | Command | Expected | Status |
|---|---|---|---|
| probe div | `await browser.nevoflux.probe(null, '#tgt')` | `is_content_editable === true`, `editor_framework === null` | [ ] |
| fillRichText | `await browser.nevoflux.fillRichText(null, '#tgt', 'Hello world')` | Div shows "Hello world" | [ ] |
| fill routes to fillRichText | `await browser.nevoflux.fill(null, '#tgt', 'Replaced')` | Div shows "Replaced" (not silent failure) | [ ] |
| type appends | `await browser.nevoflux.type(null, '#tgt', '!!!')` | Div shows "Replaced!!!" | [ ] |
| paste inserts | `await browser.nevoflux.paste(null, '#tgt', ' added')` | Div shows "Replaced!!! added" | [ ] |

### 20.3 Draft.js (draft-js.html)

| Test | Command | Expected | Status |
|---|---|---|---|
| probe editor | `await browser.nevoflux.probe(null, '.public-DraftEditor-content')` | `editor_framework === 'draft.js'`, `react_fiber_present === true` | [ ] |
| fillRichText | `await browser.nevoflux.fillRichText(null, '.public-DraftEditor-content', 'Hello Draft')` | Editor shows "Hello Draft" | [ ] |
| fill via top selector | `await browser.nevoflux.fill(null, '#root', 'Via root')` | Editor shows "Via root" (delegates to fillRichText) | [ ] |

### 20.4 Lexical stub (lexical.html)

| Test | Command | Expected | Status |
|---|---|---|---|
| probe | `await browser.nevoflux.probe(null, '#lex')` | `editor_framework === 'lexical'` | [ ] |
| fillRichText | `await browser.nevoflux.fillRichText(null, '#lex', 'Lex text')` | Div shows "Lex text" | [ ] |

### 20.5 ProseMirror stub (prosemirror.html)

| Test | Command | Expected | Status |
|---|---|---|---|
| probe | `await browser.nevoflux.probe(null, '#pm')` | `editor_framework === 'prosemirror'` | [ ] |
| fillRichText | `await browser.nevoflux.fillRichText(null, '#pm', 'PM text')` | Div shows "PM text" | [ ] |

### 20.6 Slate stub (slate.html)

| Test | Command | Expected | Status |
|---|---|---|---|
| probe | `await browser.nevoflux.probe(null, '#slate')` | `editor_framework === 'slate'` | [ ] |
| fillRichText | `await browser.nevoflux.fillRichText(null, '#slate', 'Slate text')` | Div shows "Slate text" | [ ] |

### 20.7 queryAll no-CSP regression

On a strict-CSP site (e.g. https://x.com):

| Test | Command | Expected | Status |
|---|---|---|---|
| queryAll on strict CSP | `await browser.nevoflux.queryAll(null, 'input[type="file"]')` | Returns `{success: true, result: {count: N, elements: [...]}}` — NOT a 9001 CSP error | [ ] |
| query returns path_selector | Same command as above | Each element in `result.elements` has a non-empty `path_selector` field | [ ] |
