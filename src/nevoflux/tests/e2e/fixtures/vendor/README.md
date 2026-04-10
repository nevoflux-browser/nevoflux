# Vendor Files for Editor Fixtures

These minified third-party scripts support the manual editor fixtures
in `../editors/`. Versions are pinned for determinism.

| File                             | Version | Source (for update) |
|----------------------------------|---------|---------------------|
| react.production.min.js          | 18.2.0  | unpkg.com/react@18.2.0 |
| react-dom.production.min.js      | 18.2.0  | unpkg.com/react-dom@18.2.0 |
| draft.min.js                     | 0.11.7  | unpkg.com/draft-js@0.11.7 |
| draft.min.css                    | 0.11.7  | unpkg.com/draft-js@0.11.7 |

## Not yet vendored

- Lexical — requires module bundling; fixture uses contenteditable stub
- Slate — requires React setup; fixture uses contenteditable stub
- CodeMirror, Monaco, Quill, TinyMCE — deferred to later PRs

## Update policy

Update versions only when a fixture test breaks or when targeting a
newer framework API. Preserve reproducibility.
