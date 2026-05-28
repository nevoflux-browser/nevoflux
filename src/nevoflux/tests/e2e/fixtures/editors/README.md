# Editor Fixtures — Manual Verification

Static HTML pages that exercise the NevoFlux Actor primitives against
real (or realistic) editor framework DOM structures. Used for manual
verification during implementation and smoke testing.

## How to run

1. Build the browser: `npm run build` (rebuilds omni.ja with the latest
   built-in nevoflux-agent payload — there is no separate "reload extension"
   step for built-in WebExtensions; the jar manifest is baked into omni.ja).
   For UI-only changes, `npm run build:ui` is faster.
2. Start the browser: `npm run start`
4. Open the fixture: e.g.
   `file:///ai/project/nevoflux/src/nevoflux/tests/e2e/fixtures/editors/draft-js.html`
5. Open the browser console (Ctrl+Shift+J)
6. Run the verification commands shown in each fixture page

## Fixtures

| File | Covers |
|------|--------|
| standard-input.html | INPUT / TEXTAREA baseline (no regression) |
| simple-contenteditable.html | Bare contenteditable div |
| draft-js.html | Draft.js with pinned React + Draft.js build |
| lexical.html | Lexical framework detection (stub) |
| prosemirror.html | ProseMirror framework detection (stub) |
| slate.html | Slate framework detection (stub) |

## What to check

For each fixture, verify:

1. `browser.nevoflux.probe(null, '<selector>')` returns the expected
   fingerprint fields (is_content_editable, editor_framework, etc.).
2. `browser.nevoflux.fillRichText(null, '<selector>', 'test text')`
   succeeds and the text is visible in the editor.
3. `browser.nevoflux.fill(null, '<selector>', 'second text')`
   replaces the content (contentEditable branch).
4. `browser.nevoflux.type(null, '<selector>', ' append')` appends
   characters one at a time.
5. `browser.nevoflux.paste(null, '<selector>', ' paste')` inserts
   at the current cursor.
6. `browser.nevoflux.queryAll(null, 'input[type="file"]')` on any
   page returns a list without a CSP error.

Record pass/fail in `../E2E_TEST_CHECKLIST.md` under the new section.
