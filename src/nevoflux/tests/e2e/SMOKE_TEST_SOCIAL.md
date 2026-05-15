# Browser Input — Social Media Smoke Test Checklist

Manual checklist run before each release. Not CI, not automated.

## Prerequisites

- NevoFlux browser, native agent, and agent panel built and running (`npm run start:full`)
- Sidebar open, agent responding

## X.com (Twitter)

### Text Input
- [ ] Navigate to x.com home
- [ ] Agent: `browser_input` with `selector: '[data-testid="tweetTextarea_0"]'`, `text: "Hello from NevoFlux"`, `mode: "fill"`
- [ ] Verify: compose box contains "Hello from NevoFlux"
- [ ] Verify: response `strategy_used` = `rich_text_fill`
- [ ] Verify: response `framework_detected` = `draft.js`

### @Mention Flow
- [ ] Agent: `browser_input` with `text: "Hey @nevoflux test mention"`
- [ ] Verify: mention candidate list appeared
- [ ] Verify: Enter confirmed the first candidate
- [ ] Verify: response `strategy_used` = `sequence`

### Image Upload (requires PR #5)
- [ ] Skip until PR #5 is merged
- [ ] Agent: `browser_upload_file` with `selector: 'input[data-testid="fileInput"]'`
- [ ] Verify: image thumbnail appears in compose

### Submit
- [ ] Agent: `browser_click` with `selector: '[data-testid="tweetButtonInline"]'`
- [ ] Verify: tweet posted (delete after testing)

## LinkedIn (or Reddit)

### Text Input
- [ ] Navigate to linkedin.com feed (or reddit.com subreddit)
- [ ] Agent: `browser_input` with compose selector, `text: "Test post"`, `mode: "fill"`
- [ ] Verify: text appears in compose box
- [ ] **Do NOT submit** (no need to post on LinkedIn/Reddit during smoke testing)

## Discord

### Text Input
- [ ] Navigate to a test server text channel
- [ ] Agent: `browser_input` with `selector: 'div[role="textbox"]'`, `text: "Hello Discord"`, `mode: "fill"`
- [ ] Verify: text appears in message box
- [ ] Agent: `browser_input` with `text: " more text"`, `mode: "type"` (append)
- [ ] Verify: message box now contains "Hello Discord more text"

## browser_eval_js Error Handling

### CSP Error (9001)
- [ ] On x.com: `browser_eval_js` with `expression: "document.title"`
- [ ] Verify: error code 9001, message contains "CSP" or "Content Security Policy"
- [ ] Verify: hint suggests structured tool alternative

### Runtime Error (9004)
- [ ] On a non-CSP site: `browser_eval_js` with `expression: "undeclaredVar.foo"`
- [ ] Verify: error code 9004, `recoverable: true`

## Regression Checks

- [ ] `browser_fill_by_id` on a standard `<input>` still works (not broken by deprecation)
- [ ] `browser_type_by_id` on a standard `<input>` still works
- [ ] `browser_query_all` returns results with `path_selector` field
- [ ] `browser_probe` returns a `Fingerprint` with `editor_framework` field

## Result

- Date: ___________
- Tester: ___________
- Pass / Fail: ___________
- Notes: ___________
