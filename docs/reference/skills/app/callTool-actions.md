# callTool Actions Reference

Complete reference for `NevofluxSDK.callTool(action, params)`.

All actions return `{ success: boolean, result?: object, error?: { code, message, recoverable } }`.
A refused action also carries `error.policy_code` (see **Policy refusals** below).

Most actions accept an optional `tab_id` param to target a specific tab. Reads
default to the active tab; **writes must target a tab this panel opened** —
see **Which tabs a panel may drive**.

**Naming convention**: most actions use `snake_case` (`get_markdown`, `list_tabs`). A few newer actions use `camelCase` and are called out in each table: `activateTab`, `fillRichText`, `uploadFile`. Match the exact casing shown.

**Tab-independent actions** (don't require an open web tab): `web_fetch`, `web_search`, `ask_user`, `list_tabs`, `query_tabs`, `read_artifact`, `edit_artifact`, `canvas_render`, `cache_file`, `cache_tab_markdown`.

## Navigation

| Action        | Params                         | Result                                                                  |
| ------------- | ------------------------------ | ----------------------------------------------------------------------- |
| `navigate`    | `{ url, new_tab? }`            | `{ url, tab_id, new_tab }`                                              |
| `activateTab` | `{ tab_id }`                   | `{ tab_id, url, title }`                                                |
| `go_back`     | `{}`                           | `{ url }`                                                               |
| `go_forward`  | `{}`                           | `{ url }`                                                               |

- `new_tab`: `true` opens in a new tab, `false`/omitted navigates in place (falls back to any web tab if current tab isn't web)
- `activateTab` (camelCase) switches focus to an existing tab — use with `tab_id` from `list_tabs`/`query_tabs`

## Interaction (selector-based)

| Action      | Params                                | Result                        |
| ----------- | ------------------------------------- | ----------------------------- |
| `click`     | `{ selector, button?, click_count? }` | `{ selector, clicked: true }` |
| `type`      | `{ selector, text }`                  | `{ selector, text }`          |
| `fill`      | `{ selector, value }`                 | `{ selector, value }`         |
| `key_press` | `{ key, modifiers? }`                 | `{ key, navigated? }`         |
| `scroll`    | `{ direction?, amount? }`             | `{ scrolled: true }`          |

- `button`: `"left"` (default), `"right"`, `"middle"`
- `click_count`: number of clicks (default 1, use 2 for double-click)
- `direction`: `"up"`, `"down"` (default `"down"`)
- `amount`: `"page"` (default) or pixel count
- `modifiers`: array of `"Ctrl"`, `"Shift"`, `"Alt"`, `"Meta"`
- All click/type/fill/key_press use **trusted events** (`isTrusted=true`), bypassing CSP restrictions

## Rich Text / ContentEditable

Use for rich text editors (Google Docs, Notion, ProseMirror, Slate, Lexical, etc.) where plain `type`/`fill` fails because the target is a `contenteditable` div, not a native input.

| Action         | Params                 | Result                         |
| -------------- | ---------------------- | ------------------------------ |
| `paste`        | `{ selector, text }`   | `{ selector, pasted: true }`   |
| `fillRichText` | `{ selector, text }`   | `{ selector, filled: true }`   |

- `paste` inserts text at the current caret position via the clipboard/execCommand paste path — preserves existing content
- `fillRichText` (camelCase) clears the element first, then pastes — the "replace all" variant
- Both use privileged actor methods that bypass CSP clipboard restrictions

## File Upload

| Action       | Params                                          | Result                                  |
| ------------ | ----------------------------------------------- | --------------------------------------- |
| `uploadFile` | `{ selector, fileUrl, fileName?, mimeType? }`   | `{ selector, uploaded: true, fileName }` |

- `uploadFile` (camelCase) attaches a file to an `<input type="file">` element. `fileUrl` is typically a result path from `cache_file` (e.g. `file:///tmp/cached/xxx.png`) or any URL the browser can fetch
- `fileName` (optional): display name shown in the form (default `"upload"`)
- `mimeType` (optional): MIME type hint (default `"application/octet-stream"`)
- Uses a localhost HTTP bridge + `mozSetFileArray` to work around Xray wrapper issues with `DataTransfer`

## Interaction (ID-based, after snapshot)

| Action        | Params                                | Result                                     |
| ------------- | ------------------------------------- | ------------------------------------------ |
| `click_by_id` | `{ element_id }`                      | `{ element_id, clicked: true, effective }` |
| `type_by_id`  | `{ element_id, text, press_enter? }`  | `{ element_id, typed, enter_pressed }`     |
| `fill_by_id`  | `{ element_id, value, press_enter? }` | `{ element_id, filled, enter_pressed }`    |

- Element IDs come from a prior `snapshot` call (e.g. `"e1"`, `"e2"`)
- `press_enter`: auto-press Enter after typing/filling (default `false`)
- `click_by_id` detects click effectiveness via DOM changes and network requests
- Falls back to coordinate-based click if CSS selector fails

## Content Reading

| Action         | Params                 | Result                                              |
| -------------- | ---------------------- | --------------------------------------------------- |
| `get_content`  | `{ selector? }`        | `{ text }` — text of element or full page           |
| `get_markdown` | `{ max_length? }`      | `{ markdown, title, url }`                          |
| `snapshot`     | `{}`                   | `{ tree, refs, element_count, stats, url, title }`  |
| `get_elements` | `{}`                   | Alias for `snapshot`                                |
| `get_element`  | `{ selector }`         | `{ selector, exists, visible }`                     |
| `query_all`    | `{ selector, limit? }` | `{ count, elements: [{ tag, id, text, visible }] }` |
| `screenshot`   | `{ full_page? }`       | `{ data_url, width, height, full_page }`            |

- `snapshot` returns an accessibility tree + element refs map for `*_by_id` actions
- Refs persist 60s per entry, 5min total per tab; new snapshots merge into existing refs
- `query_all` default `limit`: 50
- `screenshot` returns base64-encoded PNG as `data_url`

## Tab Management

| Action       | Params                                 | Result                                                            |
| ------------ | -------------------------------------- | ----------------------------------------------------------------- |
| `list_tabs`  | `{}`                                   | `{ tabs: [{ id, url, title, active, index, windowId, status }] }` |
| `query_tabs` | `{ url?, title?, active?, windowId? }` | `{ tabs: [...] }`                                                 |

`url` and `title` support glob patterns (e.g. `"https://github.com/*"`).

## Waiting

| Action            | Params                              | Result                              |
| ----------------- | ----------------------------------- | ----------------------------------- |
| `wait_for`        | `{ selector, state?, timeout_ms? }` | `{ selector, appeared }`            |
| `wait_for_stable` | `{ strategy?, maxWait? }`           | `{ stable, strategy, duration_ms }` |

- `state`: `"visible"` (default), `"hidden"`, `"attached"`, `"detached"`
- `timeout_ms`: milliseconds (default 30000)
- `strategy`: `"navigation"`, `"interaction"`, or `"scroll"` (default: auto-detect)
- `maxWait`: milliseconds (default 3000)

## JavaScript Execution

| Action    | Params       | Result                                   |
| --------- | ------------ | ---------------------------------------- |
| `eval_js` | `{ script }` | Return value of the evaluated expression |

Runs in the context of the active browser tab, not the artifact iframe. Many sites block via CSP — prefer `get_markdown` for content reading.

## Web

| Action       | Params                                                               | Result                                                         |
| ------------ | -------------------------------------------------------------------- | -------------------------------------------------------------- |
| `web_fetch`  | `{ url, timeout_ms?, max_length?, include_images?, force_refresh? }` | `{ file_path, url, title, content_length, cached }`            |
| `web_search` | `{ query, max_results?, timeout_ms? }`                               | `{ results: [{ title, url, snippet }], query, total_results }` |

- `web_fetch` converts page to markdown and caches locally. Returns `file_path` for the agent to read.
- `max_length`: default 100000. Content auto-truncated if larger.
- `include_images`: include image references in markdown (default `false`)
- `force_refresh`: bypass cache (default `false`)
- `web_search` uses DuckDuckGo, default `max_results`: 10

## User Interaction

| Action     | Params                                               | Result                                  |
| ---------- | ---------------------------------------------------- | --------------------------------------- |
| `ask_user` | `{ question, options?, allow_custom?, timeout_ms? }` | `{ answer, is_custom, selected_index }` |

- Displays a prompt dialog in the sidebar
- `options`: array of choice strings for the user to pick from
- `allow_custom`: let user type a custom answer (default `true`)
- `timeout_ms`: auto-cancel after timeout (default 60000)
- `selected_index`: index of chosen option, or `-1` for custom answers

## Artifact Management

| Action          | Params                                     | Result                                            |
| --------------- | ------------------------------------------ | ------------------------------------------------- |
| `read_artifact` | `{ id, offset?, limit?, grep?, context? }` | `{ content, totalLines, truncated, title, type }` |
| `edit_artifact` | `{ id, old_str, new_str }`                 | `{ lines }`                                       |
| `canvas_render` | `{ files, title?, artifact_id?, entry? }`  | `{ artifact_id, url }`                            |

- `read_artifact`: reads artifact content. Auto-truncates at 1000 lines without params.
  - `offset`: 1-based line number to start from
  - `limit`: number of lines to return
  - `grep`: search keyword — returns matching lines with `context` lines around each match (default 5)
- `edit_artifact`: exact search-and-replace. Fails if `old_str` not found or matches multiple locations. Cannot edit while artifact is still streaming (error 12004).
- `canvas_render`: creates a multi-file project artifact and opens it in a canvas tab.
  - `files`: object mapping file paths to content strings (e.g. `{ "/src/App.jsx": "..." }`)
  - `entry`: entry point file path (auto-detected if omitted)

## File Caching

| Action               | Params                          | Result                                 |
| -------------------- | ------------------------------- | -------------------------------------- |
| `cache_file`         | `{ name, content, mime_type? }` | `{ file_path, name, size, mime_type }` |
| `cache_tab_markdown` | `{ max_length? }`               | `{ file_path, size, markdown }`        |

- `cache_file`: `content` should be base64-encoded for binary files. Text MIME types are auto-decoded.
- `cache_tab_markdown`: converts current tab to markdown and saves to cache file. Default `max_length`: 100000.

## Which tabs a panel may drive

A panel may read from any tab, but it may only **write** to a tab it opened
itself. Reaching into the tab the user happens to be looking at is a different
act from driving your own, and it is refused.

So the first thing an interactive panel does is open its own tab:

```js
const { result } = await NevofluxSDK.callTool('navigate', {
  url: 'https://example.com',
  new_tab: true,          // exempt from the ownership rule -- this is how you get a tab
});
const myTab = result.tab_id;

// From here, pass that tab_id explicitly rather than relying on the default.
await NevofluxSDK.callTool('click', { tab_id: myTab, selector: '#go' });
```

Writing without `tab_id` falls back to the active tab, which is usually **not**
yours, and returns `TAB_NOT_OWNED`. Read actions are unaffected.

Tabs are forgotten when they close.

## Declaring what a pack panel may reach

A panel shipped by a pack reaches **only** the actions its manifest declares:

```toml
[components.dashboard]
artifact_id  = "mypack-dashboard"
content_type = "text/html"
files_from   = "panel"
entry        = "index.html"
capabilities = { call_tool = ["navigate", "get_content", "click"], invoke = [] }
```

Declaring nothing reaches nothing — an empty or absent `call_tool` list refuses
every action with `CAPABILITY_NOT_DECLARED`. That is deliberate: reviewing a
pack should tell you what its panel can do without reading its code.

A canvas that no pack owns — one you or a session created — has no declaration
to enforce and is not subject to this. It is still subject to site rules.

## Policy refusals

An installed pack can restrict what happens on particular sites. A refused call
returns `success: false` with a `policy_code`:

| `policy_code`             | Meaning                                                                 |
| ------------------------- | ----------------------------------------------------------------------- |
| `POLICY_DENIED`           | A pack's rule forbids this action here. `message` is the pack's own text |
| `TAB_NOT_OWNED`           | Write aimed at a tab this panel did not open                            |
| `CAPABILITY_NOT_DECLARED` | Action absent from this panel's `dashboard.capabilities.call_tool`       |
| `POLICY_UNAVAILABLE`      | The agent could not be reached to check; writes refuse, reads proceed    |

These are decisions, not failures: they arrive with `recoverable: false` and
retrying changes nothing. Show the `message` and offer the user another route.

## Error Codes

| Range         | Category                                                                     |
| ------------- | ---------------------------------------------------------------------------- |
| `6001-6004`   | web_fetch errors (fetch failed, unsupported content-type, content too large) |
| `7001-7002`   | web_search errors (invalid query, search failed)                             |
| `8001`        | ask_user errors (timeout, user cancelled)                                    |
| `12004`       | Artifact still streaming (cannot edit)                                       |
| `12005-12006` | Artifact edit errors (string not found, multiple matches)                    |
