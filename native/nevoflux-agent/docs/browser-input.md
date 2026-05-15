# Browser Input System

The browser input system provides reliable text input across standard form fields (`<input>`, `<textarea>`) and rich-text editors (Draft.js, Lexical, ProseMirror, Slate, CodeMirror, Monaco, Quill, TinyMCE).

## Architecture

```
LLM (agent)
  │ browser_input / browser_probe
  ▼
Rust Daemon (strategy engine)
  │ decide() → ExecutionPlan
  ▼
Extension (background.js)
  │ native messaging dispatch
  ▼
NevofluxChild Actor (chrome-privileged)
  │ probe / fill / fillRichText / paste / queryAll
  ▼
Page DOM
```

**Key principle:** Strategy decisions live in Rust (`decide()` is a pure function over `Fingerprint + hostname + Recipe`). Execution primitives live in the chrome-privileged Actor layer, immune to page CSP.

## Tools

### `browser_input` (primary)

High-level input tool. Probes the target, selects a strategy, executes, and verifies.

```
browser_input(selector, text, mode="fill", verify=true, tab_id=None)
```

- **mode `fill`**: Replace existing content (like clearing + typing).
- **mode `type`**: Append to existing content.
- Returns: strategy used, framework detected, verification result, fingerprint snapshot.

### `browser_probe` (diagnostic)

Returns a `Fingerprint` without performing any input. Useful when `browser_input` fails and you need to reason about the element.

```
browser_probe(selector, tab_id=None)
```

### `browser_fill_by_id` / `browser_type_by_id` (deprecated)

Snapshot-based input tools. **Deprecated 2026-04** — prefer `browser_input` which handles rich-text editors. These tools remain for backward compatibility.

### `browser_eval_js` (escape hatch)

Runs arbitrary JS in a content-principal sandbox. Subject to page CSP — strict sites (Twitter/X, GitHub) will block it. Prefer structured tools whenever possible.

## Strategy Engine

The `decide()` function in `crates/daemon/src/agent/browser_input/strategy.rs` selects a plan based on:

1. **Rejection checks**: disabled, readonly, invisible → `Abort`.
2. **Platform adapter**: if a recipe matches the hostname and text contains mentions → `Sequence` (segmented mention flow).
3. **contentEditable**: if the element is editable → `RichTextFill` (fill mode) or `Paste` (type mode), targeting the innermost editable child.
4. **Standard input**: if `.value` property exists → `NativeFill`.
5. **Unknown**: no supported path → `Abort`.

## Platform Adapter (Recipes)

YAML recipes provide per-site knowledge without hardcoding site-specific logic.

**Load order** (first match wins):
1. `~/.config/nevoflux/recipes/*.yaml` — user overrides
2. `$SHARE/nevoflux/recipes/*.yaml` — release-bundled (if present)
3. Compiled-in fallback (`crates/daemon/recipes/x_com.yaml`)

**Shipped recipe:** `x_com.yaml` (x.com / twitter.com) — handles `@mention` flows.

### Adding a new recipe

Create a `.yaml` file in `~/.config/nevoflux/recipes/` following the schema:

```yaml
name: my_site
hostname_patterns: ["example.com"]
version: 1
enabled: true

compose:
  selector: '#compose-box'

submit:
  selector: '#submit-btn'

mention:
  trigger_char: "@"
  pattern: '@([A-Za-z0-9_]+)'
  candidate_list_selector: '.mention-list'
  candidate_list_timeout_ms: 2000
  confirm_method: "enter_key"  # or "click_first"
```

Validation enforced on load:
- `#[serde(deny_unknown_fields)]` — unknown YAML keys rejected
- `mention.pattern` must be a valid regex
- `candidate_list_timeout_ms` ≤ 10,000ms
- `upload_complete_timeout_ms` ≤ 300,000ms
- Hostname patterns are literal strings (no regex — prevents ReDoS)

## Error Codes

| Code | Meaning | Recoverable |
|------|---------|-------------|
| 1001 | Element not found | yes |
| 1002 | Could not focus target | yes |
| 1003 | Not `<input type="file">` | no |
| 1007 | Invalid CSS selector | no |
| 1008 | Element disabled/readonly | no |
| 1009 | Editor framework not supported | no |
| 1014 | Verification mismatch | yes |
| 9001 | CSP blocked eval (terminal) | no |
| 9004 | JS runtime/syntax error | yes |

## Verification

By default, `browser_input` reads back the element content after execution and compares it to the expected text. Mismatch diagnostics include:
- Empty actual → "input events may not have been dispatched"
- Starts with "undefined" → "value-setter bug on contentEditable"
- Much longer than expected → "previous content not cleared"
- Expected `@` but actual doesn't have it → "mention flow missing; needs recipe"

## File Layout

```
crates/daemon/src/agent/browser_input/
├── mod.rs              # run_browser_input / run_browser_probe orchestration
├── bridge.rs           # BrowserBridge trait + RealBrowserBridge
├── error.rs            # BrowserInputError enum
├── executor.rs         # ExecutionPlan → Actor method sequence
├── fingerprint.rs      # Fingerprint + EditorFramework types
├── plan.rs             # ExecutionPlan + Action + InputMode enums
├── platform_adapter.rs # Recipe types, AdapterRegistry, loader
├── strategy.rs         # decide() pure function + apply_recipe
└── verifier.rs         # Read-back verification

crates/daemon/recipes/
└── x_com.yaml          # Compiled-in X.com recipe
```
