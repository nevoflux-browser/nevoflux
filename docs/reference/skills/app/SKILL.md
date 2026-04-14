---
name: app
description: Build interactive Canvas apps and visual outputs using NevofluxSDK. Supports HTML single-page apps, React components, multi-file projects (bundled with esbuild), SVG graphics, Mermaid diagrams, and Markdown documents — all with persistent storage, browser control, AI agent interaction, event bus pub/sub, whitelisted CLI tool invocation (ffmpeg/git/fs/etc.), and end-to-end encrypted sharing. Use this skill whenever the user wants to create any visual app, dashboard, widget, tool, game, calculator, chart, diagram, data visualization, interactive UI, or any output that should be rendered visually — even if they don't say "canvas" or "artifact" explicitly.
version: 1.2.2
tags:
  - canvas
  - artifact
  - html
  - react
  - project
  - app
  - dashboard
  - visualization
  - diagram
  - svg
  - mermaid
  - events
  - eventbus
  - pubsub
  - tool
  - ffmpeg
  - share
  - collaboration
enabled: true
triggers:
  - dashboard
  - calculator
  - todo app
  - widget
  - chart
  - diagram
  - visualization
  - game
  - interactive
  - mermaid
  - svg
  - 仪表盘
  - 可视化
  - 图表
  - 小工具
  - 小游戏
  - 做个app
  - 做个工具
allowed_tools: []
---

# Canvas App Builder

You build interactive Canvas applications using the `create_artifact` tool. Every artifact iframe has `window.NevofluxSDK` auto-injected, providing persistent storage, browser control, AI agent interaction, and sidebar messaging.

## Choosing Content Type

Pick the simplest type that fits:

| Type         | `content_type`    | When to Use                                           |
| ------------ | ----------------- | ----------------------------------------------------- |
| **HTML**     | `"text/html"`     | Default. Single-page apps, tools, dashboards, games   |
| **React**    | `"react"`         | 3+ independent components or complex state management |
| **Project**  | `"project"`       | Multi-file apps needing module imports and bundling   |
| **SVG**      | `"image/svg+xml"` | Vector graphics, icons, illustrations                 |
| **Mermaid**  | `"text/mermaid"`  | Flowcharts, sequence diagrams, ER diagrams            |
| **Markdown** | `"text/markdown"` | Formatted text, documentation, reports                |

**Decision guide**: Start with HTML. Move to React if you have 3+ components with shared state. Move to project only when you need separate files with imports between them.

## create_artifact Tool

**Single-file** (HTML / React / SVG / Mermaid / Markdown):

```json
{
  "title": "My Dashboard",
  "content_type": "text/html",
  "content": "<!DOCTYPE html><html>...</html>"
}
```

**Multi-file project** (bundled with esbuild):

```json
{
  "title": "Task Manager",
  "content_type": "project",
  "entry": "/src/App.jsx",
  "files": {
    "/src/App.jsx": "import { Header } from './Header';\nexport default function App() { ... }",
    "/src/Header.jsx": "export function Header() { return <h1>Tasks</h1>; }",
    "/src/styles.css": ".card { padding: 12px; }"
  }
}
```

Parameters:

- `title` (string, required) — Display name for the artifact
- `content_type` (string, required) — One of the types from the table above
- `content` (string) — Full source for single-file artifacts
- `files` (object) — Path-to-content map for multi-file projects. Paths must start with `/`
- `entry` (string) — Entry point file path for projects (e.g. `"/src/App.jsx"`)

Use either `content` (single-file) or `files` + `entry` (project), not both.

## Rules

- **Default to HTML.** Single inline file covers most use cases.
- **HTML apps**: Include all JS/CSS inline. No external resources.
- **React apps**: Output **only JSX code** — no `<html>`, `<head>`, `<script>` wrappers. React, ReactDOM, and Babel are auto-injected by the canvas.
- **Project apps**: Each file path must start with `/`. Use standard ES module imports between files. The canvas bundles everything with esbuild.
- **No CDN/external URLs in HTML apps.** `<script src="https://...">` will NOT load. The iframe sandbox blocks external network requests. React/Babel are provided by the canvas — never import them manually.
- **No inline event handlers** (`onclick="..."`). Use `addEventListener` or React's `onClick`.
- `callTool` returns `{ success, result, error }` — always check `success` before using `result`.

## NevofluxSDK API

### Storage (persistent key-value)

```javascript
await NevofluxSDK.storage.get(key); // Returns value or null
await NevofluxSDK.storage.set(key, value); // Persist JSON-serializable value
await NevofluxSDK.storage.delete(key); // Remove key
await NevofluxSDK.storage.query(prefix); // List keys by prefix
```

Use app-specific key prefixes (e.g. `todo:items`, `dashboard:config`) to avoid collisions.

### Browser Tools (via callTool)

```javascript
await NevofluxSDK.callTool(action, params);
```

Most action names use **snake_case**; a few use camelCase (flagged below). Common actions:

| Action          | Params                                        | Purpose                                      |
| --------------- | --------------------------------------------- | -------------------------------------------- |
| `navigate`      | `{ url, new_tab? }`                           | Navigate (optionally open new tab)           |
| `activateTab`   | `{ tab_id }`                                  | Switch focus to existing tab (camelCase)     |
| `get_markdown`  | `{}`                                          | Get page content as markdown                 |
| `screenshot`    | `{}`                                          | Capture page screenshot                      |
| `list_tabs`     | `{}`                                          | List all open tabs                           |
| `query_tabs`    | `{ url?, title?, active? }`                   | Filter tabs                                  |
| `get_elements`  | `{}`                                          | Get interactive page elements (alias)        |
| `snapshot`      | `{}`                                          | Full a11y tree + element refs for `*_by_id`  |
| `click`         | `{ selector }`                                | Click element                                |
| `type`          | `{ selector, text }`                          | Type into element                            |
| `fill`          | `{ selector, value }`                         | Fill native input                            |
| `paste`         | `{ selector, text }`                          | Insert into contenteditable at caret         |
| `fillRichText`  | `{ selector, text }`                          | Clear + fill contenteditable (camelCase)     |
| `uploadFile`    | `{ selector, fileUrl, fileName?, mimeType? }` | Attach file to `<input type=file>` (camelCase)|
| `web_search`    | `{ query }`                                   | Search the web                               |
| `web_fetch`     | `{ url }`                                     | Fetch URL content                            |
| `eval_js`       | `{ code }`                                    | Run JS in active browser tab                 |
| `ask_user`      | `{ question, options? }`                      | Prompt user in sidebar                       |

Full action reference: load auxiliary file `callTool-actions.md`.

### Agent (bidirectional AI interaction)

`agent.chat()` sends a message to the AI agent and returns a Promise that resolves with the full response when the agent finishes. It supports streaming callbacks for real-time feedback and attachments (images, files, directories).

**Simple (await final result):**

```javascript
const result = await NevofluxSDK.agent.chat('summarize this page');
// result = { text: "...", toolResults: [...], sessionId: "cs_..." }
```

**With attachments:**

```javascript
// Send image (base64 encoded)
const result = await NevofluxSDK.agent.chat('describe this image', {
  attachments: [
    { name: 'photo.png', mime_type: 'image/png', data: base64String }
  ],
});

// Send local files or directories
const result = await NevofluxSDK.agent.chat('analyze this codebase', {
  attachments: [
    { path: '/home/user/project', is_directory: true },
    { path: '/home/user/data.csv' },
  ],
});

// Mixed: images + files in one request
const result = await NevofluxSDK.agent.chat('compare the screenshot with the source', {
  attachments: [
    { name: 'screenshot.png', mime_type: 'image/png', data: base64String },
    { path: '/home/user/index.html' },
  ],
});
```

Attachment types:

| Field          | Type    | Description                                    |
| -------------- | ------- | ---------------------------------------------- |
| `name`         | string  | Display name (for images)                      |
| `mime_type`    | string  | MIME type, e.g. `"image/png"` (for images)     |
| `data`         | string  | Base64 encoded content (for images)            |
| `path`         | string  | Absolute file/directory path (for local files) |
| `is_directory` | boolean | `true` if path is a directory (default: false) |

Use `{ name, mime_type, data }` for images. Use `{ path }` or `{ path, is_directory: true }` for local files/directories.

**Streaming (real-time callbacks):**

```javascript
const result = await NevofluxSDK.agent.chat('analyze the data', {
  onStream: (chunk) => {
    // { type: "text", delta: "I'll start by..." }
    outputEl.textContent += chunk.delta;
  },
  onToolResult: (toolResult) => {
    processData(toolResult);
  },
  onState: (state) => {
    // { status: "thinking" | "tool_executing" | "streaming" | "idle" }
    statusEl.textContent = state.status;
  },
});
```

**Multi-turn conversation:**

```javascript
const r1 = await NevofluxSDK.agent.chat('find all images on this page');
const r2 = await NevofluxSDK.agent.chat('now download the first one', {
  sessionId: r1.sessionId, // continues same conversation
});
```

**Cancel a running request:**

```javascript
NevofluxSDK.agent.cancel(sessionId);
```

**System commands:**

```javascript
await NevofluxSDK.agent.sendCommand(command, params);
```

MCP tools are not directly callable. Use `agent.chat()` to ask the agent to invoke them.

### Sidebar

```javascript
await NevofluxSDK.sidebar.open(); // Open sidebar panel
await NevofluxSDK.sidebar.send(message); // Send message to sidebar
await NevofluxSDK.sidebar.notify(type, data); // Typed notification
```

### System

```javascript
await NevofluxSDK.system.getInfo(); // Get browser/system info
```

### Events (EventBus pub/sub)

Publish and subscribe to events across sessions/canvases. Three delivery modes:

- **ephemeral** — fire-and-forget, not retained
- **sticky** — last value cached per topic, delivered to new subscribers immediately
- **persistent** — written to SQLite (queryable via `history()`)

Topic format: colon-separated segments matching `[a-zA-Z0-9_-]{1,64}`, max 8 segments (e.g. `"task:progress"`, `"session:abc:notification"`). Wildcards: `*` matches one segment (e.g. `"session:*:notification"`).

```javascript
// Subscribe (handler fires for every matching event)
const sub = await NevofluxSDK.events.subscribe(
  ['task:progress', 'session:*:notification'],
  (event) => {
    console.log(event.topic, event.payload);
  },
  { replaySticky: true, bufferSize: 256 }
);
// Later:
await sub.unsubscribe();

// Publish
await NevofluxSDK.events.publish('task:progress', { percent: 42 });
await NevofluxSDK.events.publish('config:theme', { mode: 'dark' }, { delivery: 'sticky' });
await NevofluxSDK.events.publish('log:error', { msg: '...' }, {
  delivery: { persistent: { ttlSecs: 3600 } }
});

// Query persistent history
const { events } = await NevofluxSDK.events.history('log:error', {
  limit: 100,
  sinceMs: Date.now() - 3600_000,
});

// Wait for a single event (with timeout)
const ready = await NevofluxSDK.events.waitFor('signal:ready', { timeoutMs: 30000 });

// Recover subscriptions after tab discard (call on load if your app uses events)
await NevofluxSDK.events.recover();
```

Permission matrix (topic prefix → who can publish / subscribe):

| Prefix    | Publish        | Subscribe      |
| --------- | -------------- | -------------- |
| `task:*`  | daemon         | agent + daemon |
| `agent:*` | agent + daemon | agent + daemon |
| `ui:*`    | extension      | extension      |
| `system:*`| daemon         | all            |
| `mcp:*`   | mcp server     | agent + daemon |
| `wasm:*`  | wasm plugin    | agent + daemon |

Canvases publish as extensions, so use `ui:*` or unreserved topics for your own app events.

### Canvas Tools (whitelisted CLI invocation)

**Namespace is singular: `NevofluxSDK.tool`, NOT `NevofluxSDK.tools`.** Typing the plural form silently fails with `TypeError: Cannot read properties of undefined`.

Invoke pre-configured CLI commands (ffmpeg, git, ffprobe, fs, ...) from canvas. Tools must be **registered in a TOML file** and **enabled** before they can be invoked. Configuration lives at:

- Builtin (ships with daemon): tools the user can't disable the existence of
- User-level: `~/.config/nevoflux/canvas-tools/*.toml` (one TOML per tool)
- Session-level: dynamic, in-memory only

Priority: **Builtin < User < Session** (user/session override builtin by name). Manage at `nevoflux://settings` → "Canvas Tools".

#### API

```javascript
// Check what's available FIRST — never assume a tool exists
const { tools } = await NevofluxSDK.tool.list();
// tools: [{ name, description, kind, args_mode, enabled, source }, ...]
// kind: "command" (external CLI) or "internal" (daemon API)
// source: "builtin" | "user" | "session"

const hasFfmpeg = tools.some(t => t.name === 'ffmpeg.trim' && t.enabled);
if (!hasFfmpeg) {
  // Graceful fallback — don't call a tool that isn't registered
}

// Invoke with streaming events
await NevofluxSDK.tool.invoke('ffmpeg.trim', {
  input: '$SESSION_DIR/in.mp4',
  start: '00:00:10',
  end: '00:00:20',
  output: '$SESSION_DIR/out.mp4',
}, {
  timeoutMs: 120000,
  onEvent: (event) => {
    // event_type: "started" | "stdout" | "stderr" | "progress" | "finished" | "error"
    if (event.event_type === 'stdout') appendLog(event.data);
    if (event.event_type === 'stderr') appendLog('[err] ' + event.data);
    if (event.event_type === 'progress') setProgress(event.progress);
    if (event.event_type === 'finished') setStatus('done');
    if (event.event_type === 'error') setStatus('error: ' + event.error);
  },
});
```

`invoke()` resolves with `{ callId, pending: true }` when the server accepts the request. The actual execution result arrives through `onEvent` callbacks — don't wait for the Promise value to carry the output.

#### Built-in tools (example set, may vary per deployment)

| Tool            | Backend   | Purpose                              | Example Params                              |
| --------------- | --------- | ------------------------------------ | ------------------------------------------- |
| `ffmpeg.trim`   | command   | Trim video/audio to time range       | `{ input, start, end, output }`             |
| `ffmpeg.probe`  | command   | Media metadata (duration/codecs)     | `{ input }`                                 |
| `git`           | command   | Read-only git ops                    | `{}` + `args: ['status', '--short']` (free)  |
| `fs.read`       | internal  | Read file from session directory     | `{ path, encoding? }`                       |

Path params support `$SESSION_DIR` which expands to the canvas's sandboxed directory. External tools run with `Command::new` (no shell interpretation), captured stdout/stderr, configurable timeout. Audit trail persists to SQLite.

#### Tool args modes

Tools in the whitelist use one of two `args_mode` configurations:

1. **`template`** — fixed arg structure with `{param}` placeholders. Call with `invoke(name, paramsObj)`. Each `{placeholder}` is replaced with validated param value; no free args allowed.
   Example: `ffmpeg.trim` uses `args = ["-y", "-i", "{input}", "-ss", "{start}", ...]`.

2. **`free`** — user supplies raw args array, first element must be in `allowed_subcommands`.
   Example: `git` with `allowed_subcommands = ["status", "log", "diff"]`. Call with `invoke('git', {}, { args: ['status', '--short'] })`.

Pass `free` args via the options object, not the params object — `NevofluxSDK.tool.invoke(name, params, { args: [...], onEvent, timeoutMs })`.

#### When the tool you need isn't registered

**Never call `tool.invoke()` on an unregistered tool and hope it works** — the call will fail with `Tool not found: <name>` and your canvas will look broken. Always handle this gracefully.

**Decision flow** when building a canvas that needs a CLI tool:

1. **Check first** — call `tool.list()` and verify `{ name, enabled: true }` for each tool you need.
2. **If missing or disabled, show the user a prompt** with:
   - Which tool is needed
   - Why the canvas needs it
   - Exactly what to add to the whitelist (copy-pasteable TOML block)
   - A button that opens `nevoflux://settings` (or calls `NevofluxSDK.callTool('navigate', { url: 'nevoflux://settings', new_tab: true })`)
   - A "Retry" button that re-runs `tool.list()` and resumes
3. **Alternative: offer a fallback** via `agent.chat()` for one-off needs (still informs the user that a shell command will run with the agent's broader permissions).

**Don't attempt**: `NevofluxSDK.callTool('bash', ...)` — `bash` is NOT a browser action. `callTool` only dispatches browser-side operations (navigate/click/screenshot/eval_js/web_fetch). It cannot run shell commands.

#### Missing-tool UX pattern (reusable)

Wrap any tool call with a `ensureTool()` helper. It shows the registration instructions on miss and re-checks after the user adds the TOML:

```html
<div id="tool-gate" hidden>
  <h3>⚠️ Tool required: <code id="gate-name"></code></h3>
  <p id="gate-why"></p>
  <p>Add this TOML file to <code>~/.config/nevoflux/canvas-tools/</code>:</p>
  <pre id="gate-toml"></pre>
  <button id="gate-open-settings">Open Settings</button>
  <button id="gate-retry">I've added it — Retry</button>
  <button id="gate-delegate">Use agent instead</button>
</div>

<script>
async function ensureTool(name, { why, sampleToml, onReady, onFallback }) {
  async function check() {
    const { tools } = await NevofluxSDK.tool.list();
    return tools.find(t => t.name === name && t.enabled);
  }

  const tool = await check();
  if (tool) { onReady(tool); return; }

  // Not available — show gate
  const gate = document.getElementById('tool-gate');
  document.getElementById('gate-name').textContent = name;
  document.getElementById('gate-why').textContent = why;
  document.getElementById('gate-toml').textContent = sampleToml;
  gate.hidden = false;

  document.getElementById('gate-open-settings').onclick = () => {
    NevofluxSDK.callTool('navigate', { url: 'nevoflux://settings', new_tab: true });
  };
  document.getElementById('gate-retry').onclick = async () => {
    const now = await check();
    if (now) { gate.hidden = true; onReady(now); }
    else alert('Still not found. Did you save the TOML and enable it in settings?');
  };
  document.getElementById('gate-delegate').onclick = () => {
    gate.hidden = true;
    onFallback?.();
  };
}

// Usage:
await ensureTool('ffmpeg.trim', {
  why: 'Needed to trim the uploaded video to the selected range.',
  sampleToml: `# ~/.config/nevoflux/canvas-tools/ffmpeg-trim.toml
name = "ffmpeg.trim"
description = "Trim a media file to a time range"
kind = "command"
binary = "ffmpeg"
args_mode = "template"
args = ["-y", "-i", "{input}", "-ss", "{start}", "-to", "{end}", "-c", "copy", "{output}"]

[params.input]
type = "path"
within = "$SESSION_DIR"
must_exist = true
extension = ["mp4", "mkv", "mov", "webm", "mp3", "wav"]

[params.start]
type = "duration"
min = 0.0
max = 86400.0

[params.end]
type = "duration"
min = 0.0
max = 86400.0

[params.output]
type = "path"
within = "$SESSION_DIR"

[constraints]
timeout_seconds = 120
cwd = "$SESSION_DIR"
`,
  onReady: (tool) => runTrim(tool),
  onFallback: () => {
    NevofluxSDK.agent.chat('Please run ffmpeg.trim with these args: ...');
  },
});
</script>
```

**When to generate this gate automatically**: if the canvas's core functionality depends on a tool (e.g. video editor → `ffmpeg.trim`), include the gate up front. If the tool is optional (e.g. "also run this git command"), check on-demand when the button is clicked.

**Never silently skip** the feature when the tool is missing — always explain what's happening so the user can take action.

#### Tool invocation decision table

| Goal | Use |
|------|-----|
| Click/type/read current browser tab | `NevofluxSDK.callTool('click', ...)` etc. |
| Read page text as markdown | `NevofluxSDK.callTool('get_markdown')` |
| Run whitelisted CLI (ffmpeg/git/registered tool) | `NevofluxSDK.tool.invoke('name', params, { onEvent })` |
| Run arbitrary shell / use MCP tools | `NevofluxSDK.agent.chat('please run ...')` |
| Read/write files in session dir | `fs.read` tool if registered, else delegate to agent |
| Store app state | `NevofluxSDK.storage.set(key, value)` |

### Canvas Share (encrypted link sharing)

Share artifacts as encrypted links. Content is encrypted client-side with Argon2id-derived AES-256-GCM key; the server only stores the ciphertext. Recipients need the URL AND password.

```javascript
// Share current/another artifact. Password is returned ONCE.
const shared = await NevofluxSDK.share.share(artifactId, { ttlSecs: 2592000 }); // 30d default
// {
//   share_id: "8b3g4n36k8",
//   share_url: "https://share.nevoflux.app/c/8b3g4n36k8",
//   password: "1-6XG0-GEGA-N0ET",     // 64-bit entropy, 1-4-4-4 format
//   expires_at: 1778742872,
// }

// IMPORTANT: show the password to the user immediately — we cannot recover it.
// Use the helper to copy with 60-second auto-clear:
await NevofluxSDK.share.copyPasswordWithAutoClear(shared.password, 60000);

// Import a shared canvas (usually triggered by nevoflux://import/{share_id})
const imported = await NevofluxSDK.share.import(shareId, password);
// { artifact_id, artifact_name, artifact_type, imported_from_share_id }

// List my active shares
const { shares } = await NevofluxSDK.share.list();
// [{ artifact_id, share_id, share_url, expires_at, view_count, created_at }, ...]

// Extend TTL (requires owner — no explicit token needed; stored locally encrypted)
const extended = await NevofluxSDK.share.extend(shareId, 3600);

// Delete (revokes access immediately)
await NevofluxSDK.share.delete(shareId);
```

If the canvas page loads with URL params `mode=import&share_id=xxx`, `window._nevofluxImportShareId` is set — your app can detect this and prompt the user for the password.

## HTML Template

```html
<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>My App</title>
    <style>
      * {
        box-sizing: border-box;
        margin: 0;
        padding: 0;
      }
      body {
        font-family: system-ui, sans-serif;
        padding: 20px;
        color: #1a1a1a;
      }
      .card {
        background: #fff;
        border: 1px solid #e5e5e5;
        border-radius: 8px;
        padding: 12px 16px;
        margin: 8px 0;
      }
      button {
        padding: 8px 16px;
        border: none;
        border-radius: 6px;
        background: #0066ff;
        color: #fff;
        cursor: pointer;
      }
      button:hover {
        background: #0052cc;
      }
      input {
        padding: 8px 12px;
        border: 1px solid #ddd;
        border-radius: 6px;
        outline: none;
      }
      input:focus {
        border-color: #0066ff;
        box-shadow: 0 0 0 2px rgba(0, 102, 255, 0.15);
      }
    </style>
  </head>
  <body>
    <h2>Notes</h2>
    <div style="display:flex;gap:8px;margin:12px 0">
      <input id="input" placeholder="New note..." style="flex:1" />
      <button id="add">Add</button>
    </div>
    <div id="list"></div>
    <script>
      const KEY = 'notes:items';
      async function load() {
        const items = (await NevofluxSDK.storage.get(KEY)) || [];
        render(items);
      }
      async function add() {
        const el = document.getElementById('input');
        if (!el.value.trim()) return;
        const items = (await NevofluxSDK.storage.get(KEY)) || [];
        items.push({ text: el.value.trim(), ts: Date.now() });
        await NevofluxSDK.storage.set(KEY, items);
        el.value = '';
        render(items);
      }
      function render(items) {
        document.getElementById('list').innerHTML = items
          .map((n) => `<div class="card">${n.text}</div>`)
          .join('');
      }
      document.getElementById('add').addEventListener('click', add);
      document.getElementById('input').addEventListener('keydown', (e) => {
        if (e.key === 'Enter') add();
      });
      load();
    </script>
  </body>
</html>
```

## React Template

Use when app has 3+ components or complex state. Canvas auto-injects React+Babel.
**content_type must be `"react"`.** Output ONLY JSX code — no HTML wrappers, no script tags, no React imports.

```jsx
function App() {
  const [tabs, setTabs] = React.useState([]);
  const [loading, setLoading] = React.useState(true);

  React.useEffect(() => {
    NevofluxSDK.callTool('list_tabs', {})
      .then((r) => {
        if (r.success) setTabs(r.result?.tabs || []);
      })
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <div style={{ padding: 40, textAlign: 'center' }}>Loading...</div>;

  return (
    <div style={{ fontFamily: 'system-ui', padding: 20 }}>
      <h2>Tabs ({tabs.length})</h2>
      {tabs.map((tab) => (
        <div
          key={tab.id}
          onClick={() => NevofluxSDK.callTool('navigate', { url: tab.url })}
          style={{ padding: 12, borderBottom: '1px solid #eee', cursor: 'pointer' }}>
          <strong>{tab.title}</strong>
          <div style={{ fontSize: 12, color: '#888', marginTop: 4 }}>{tab.url}</div>
        </div>
      ))}
    </div>
  );
}
```

## Agent-Driven App Template

Use when the app needs AI features — analysis, generation, Q&A, or orchestrating browser actions.

```html
<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>AI Assistant</title>
    <style>
      * {
        box-sizing: border-box;
        margin: 0;
        padding: 0;
      }
      body {
        font-family: system-ui, sans-serif;
        padding: 20px;
        max-width: 640px;
      }
      #output {
        white-space: pre-wrap;
        font-family: ui-monospace, monospace;
        background: #f5f5f5;
        padding: 16px;
        border-radius: 8px;
        min-height: 100px;
        margin: 12px 0;
        line-height: 1.5;
      }
      #status {
        font-size: 12px;
        color: #888;
        margin: 4px 0;
      }
      button {
        padding: 8px 16px;
        border: none;
        border-radius: 6px;
        background: #0066ff;
        color: #fff;
        cursor: pointer;
      }
      button:hover {
        background: #0052cc;
      }
      button:disabled {
        opacity: 0.5;
        cursor: default;
      }
    </style>
  </head>
  <body>
    <h2>Page Analyzer</h2>
    <button id="analyze">Analyze Current Page</button>
    <div id="status">Ready</div>
    <div id="output"></div>
    <script>
      const btn = document.getElementById('analyze');
      const output = document.getElementById('output');
      const status = document.getElementById('status');

      btn.addEventListener('click', async () => {
        btn.disabled = true;
        output.textContent = '';
        status.textContent = 'Thinking...';
        try {
          await NevofluxSDK.agent.chat(
            'Analyze the current page and summarize its key content in 3 bullet points',
            {
              onStream: (chunk) => {
                output.textContent += chunk.delta;
              },
              onState: (s) => {
                status.textContent = s.status;
              },
            }
          );
          status.textContent = 'Done';
        } catch (err) {
          status.textContent = 'Error: ' + err.message;
        }
        btn.disabled = false;
      });
    </script>
  </body>
</html>
```

## Event-Driven App Template

Use when multiple canvases or the sidebar need to coordinate (live dashboards, notifications, cross-tab state sync).

```html
<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>Live Counter</title>
    <style>
      body { font-family: system-ui; padding: 20px; }
      .value { font-size: 48px; font-weight: 600; color: #0066ff; }
      .meta { color: #888; font-size: 12px; }
      button { padding: 8px 16px; margin-right: 8px; border: none; border-radius: 6px;
               background: #0066ff; color: #fff; cursor: pointer; }
    </style>
  </head>
  <body>
    <h2>Live Counter</h2>
    <div class="value" id="value">0</div>
    <div class="meta" id="meta">waiting for events...</div>
    <button id="inc">+1</button>
    <button id="reset">Reset</button>
    <script>
      const TOPIC = 'ui:counter:value';
      const valueEl = document.getElementById('value');
      const metaEl = document.getElementById('meta');

      (async () => {
        // Subscribe (replaySticky=true delivers the current value immediately)
        const sub = await NevofluxSDK.events.subscribe([TOPIC], (event) => {
          valueEl.textContent = event.payload.count;
          metaEl.textContent = `from ${event.publisher} at ${new Date(event.timestamp_ms).toLocaleTimeString()}`;
        });

        document.getElementById('inc').addEventListener('click', async () => {
          const current = parseInt(valueEl.textContent, 10) || 0;
          await NevofluxSDK.events.publish(TOPIC, { count: current + 1 }, { delivery: 'sticky' });
        });

        document.getElementById('reset').addEventListener('click', async () => {
          await NevofluxSDK.events.publish(TOPIC, { count: 0 }, { delivery: 'sticky' });
        });

        // Cleanup on page unload
        window.addEventListener('unload', () => sub.unsubscribe());
      })();
    </script>
  </body>
</html>
```

## Tool-Invoking App Template

Use for apps that wrap CLI tools (video trim, git browser, file explorer).

```html
<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>Video Trimmer</title>
    <style>
      body { font-family: system-ui; padding: 20px; max-width: 520px; }
      input[type=text] { width: 100%; padding: 8px; border: 1px solid #ddd; border-radius: 6px; margin: 4px 0 12px; }
      button { padding: 10px 20px; border: none; border-radius: 6px; background: #0066ff; color: #fff; cursor: pointer; }
      button:disabled { opacity: 0.5; cursor: not-allowed; }
      #progress { width: 100%; height: 6px; background: #eee; border-radius: 3px; margin: 12px 0; }
      #bar { height: 100%; background: #0066ff; border-radius: 3px; width: 0%; transition: width 0.2s; }
      pre { background: #f5f5f5; padding: 12px; border-radius: 6px; font-size: 12px; max-height: 240px; overflow: auto; }
    </style>
  </head>
  <body>
    <h2>Video Trimmer (ffmpeg.trim)</h2>
    <label>Input path</label>
    <input id="in" value="$SESSION_DIR/video.mp4" />
    <label>Start</label>
    <input id="start" value="00:00:10" />
    <label>End</label>
    <input id="end" value="00:00:20" />
    <label>Output path</label>
    <input id="out" value="$SESSION_DIR/trimmed.mp4" />
    <button id="run">Trim</button>
    <div id="progress"><div id="bar"></div></div>
    <pre id="log"></pre>
    <script>
      document.getElementById('run').addEventListener('click', async () => {
        const btn = document.getElementById('run');
        const log = document.getElementById('log');
        const bar = document.getElementById('bar');
        btn.disabled = true;
        log.textContent = '';
        bar.style.width = '0%';

        try {
          await NevofluxSDK.tool.invoke('ffmpeg.trim', {
            input: document.getElementById('in').value,
            start: document.getElementById('start').value,
            end: document.getElementById('end').value,
            output: document.getElementById('out').value,
          }, {
            timeoutMs: 120000,
            onEvent: (e) => {
              if (e.event_type === 'stderr' || e.event_type === 'stdout') {
                log.textContent += e.data;
                log.scrollTop = log.scrollHeight;
              }
              if (e.event_type === 'progress') bar.style.width = (e.progress * 100) + '%';
              if (e.event_type === 'finished') { bar.style.width = '100%'; log.textContent += '\n[done]'; }
              if (e.event_type === 'error') log.textContent += '\n[error] ' + e.error;
            },
          });
        } catch (err) {
          log.textContent += '\n[error] ' + (err.message || err);
        }
        btn.disabled = false;
      });
    </script>
  </body>
</html>
```

## Multi-File Project Template

Use for complex apps with multiple components and module imports. The canvas bundles files with esbuild.
**content_type must be `"project"`.** Provide `files` (path-to-content map) and `entry` (root component path).

```json
{
  "title": "Task Manager",
  "content_type": "project",
  "entry": "/src/App.jsx",
  "files": {
    "/src/App.jsx": "import { TaskList } from './components/TaskList';\nimport { AddTask } from './components/AddTask';\n\nexport default function App() {\n  const [tasks, setTasks] = React.useState([]);\n  const addTask = (text) => setTasks(prev => [...prev, { id: Date.now(), text, done: false }]);\n  const toggle = (id) => setTasks(prev => prev.map(t => t.id === id ? {...t, done: !t.done} : t));\n  return (\n    <div style={{ fontFamily: 'system-ui', padding: 20, maxWidth: 500 }}>\n      <h2>Tasks</h2>\n      <AddTask onAdd={addTask} />\n      <TaskList tasks={tasks} onToggle={toggle} />\n    </div>\n  );\n}",
    "/src/components/TaskList.jsx": "export function TaskList({ tasks, onToggle }) {\n  return tasks.map(t => (\n    <div key={t.id} onClick={() => onToggle(t.id)}\n      style={{ padding: 10, cursor: 'pointer', textDecoration: t.done ? 'line-through' : 'none',\n        borderBottom: '1px solid #eee' }}>\n      {t.done ? '\\u2713' : '\\u25CB'} {t.text}\n    </div>\n  ));\n}",
    "/src/components/AddTask.jsx": "export function AddTask({ onAdd }) {\n  const [text, setText] = React.useState('');\n  const submit = () => { if (text.trim()) { onAdd(text.trim()); setText(''); } };\n  return (\n    <div style={{ display: 'flex', gap: 8, margin: '12px 0' }}>\n      <input value={text} onChange={e => setText(e.target.value)}\n        onKeyDown={e => e.key === 'Enter' && submit()}\n        placeholder='New task...' style={{ flex: 1, padding: 8, borderRadius: 6, border: '1px solid #ddd' }} />\n      <button onClick={submit}\n        style={{ padding: '8px 16px', borderRadius: 6, border: 'none', background: '#0066ff', color: '#fff', cursor: 'pointer' }}>\n        Add\n      </button>\n    </div>\n  );\n}"
  }
}
```

## Common Pitfalls

1. All `NevofluxSDK.*` calls return **Promises** — always `await` them
2. The artifact iframe cannot access the parent page DOM — communicate via NevofluxSDK only
3. Never use `onclick="..."` — use `addEventListener` or React `onClick`
4. **HTML apps**: All JS/CSS must be inline — external URLs (CDN, unpkg, etc.) are blocked by the sandbox
5. **React apps**: Use `content_type: "react"` and output ONLY JSX — no `<html>` wrappers or `<script>` tags
6. **Project apps**: File paths must start with `/`. Always provide an `entry` pointing to the root component
7. Action names are mostly **snake_case** (`get_markdown`, `list_tabs`) with a few camelCase exceptions: `activateTab`, `fillRichText`, `uploadFile`. Match the exact casing shown in the action tables
8. `callTool` returns `{ success, result, error }` — always check `success` first
9. MCP tools cannot be called directly — use `NevofluxSDK.agent.chat()` to ask the agent
10. `agent.chat()` resolves only after the agent finishes — use `onStream` callback for real-time output
11. Streaming callbacks (`onStream`, `onToolResult`, `onState`) are optional — omit for simple request-response
12. Use `sessionId` from a previous result to continue a multi-turn conversation
13. `agent.chat()` attachments: use `{ name, mime_type, data }` for base64 images, `{ path }` for local files, `{ path, is_directory: true }` for directories
14. Image `data` must be **base64 encoded** (no `data:` URI prefix) — use `btoa()` or `FileReader.readAsDataURL()` then strip the prefix
15. **Events topics** are colon-separated, `[a-zA-Z0-9_-]{1,64}` per segment, max 8 segments. Don't use dots: `"task:progress"` not `"task.progress"`
16. **Events `replaySticky: true`** (default) fires handler once per matching sticky event immediately on subscribe — handle accordingly
17. **Events `publish()` with permission errors** return EventBusResponse::Error — wrap in try/catch. Canvases cannot publish to `task:*` or `system:*`
18. **Canvas tool namespace is singular: `NevofluxSDK.tool` not `NevofluxSDK.tools`.** The plural form is `undefined` and `NevofluxSDK.tools.xxx()` throws `TypeError: Cannot read properties of undefined` — frequently silently caught by `try/catch` wrappers
19. **`bash` is NOT a browser action.** `NevofluxSDK.callTool('bash', ...)` returns `Unknown action` error because `callTool` only dispatches browser operations (navigate/click/screenshot/web_fetch). For shell commands either (a) register `bash` (or a specific command) in canvas-tools TOML and use `tool.invoke()`, or (b) delegate via `agent.chat('run ...')`
20. **Canvas tools** use snake_case in params but dotted names in tool name (e.g. `ffmpeg.trim` not `ffmpeg_trim`). Always call `tool.list()` first and verify the tool exists + is enabled before invoking — don't assume
21. **Canvas tool `onEvent` callbacks** fire multiple times per invocation; the returned Promise resolves with `{ callId, pending: true }` early — the actual result arrives through events, not the Promise value
22. **Free-mode tool args** go in the third argument (options), not the params object: `tool.invoke('git', {}, { args: ['status', '--short'], onEvent })`. Template-mode tools put values in the params (second arg)
23. **Unregistered tools must be handled gracefully.** When `tool.list()` shows the needed tool is missing or disabled, **do not silently fail or throw**. Show the user (a) which tool is needed, (b) why, (c) a copy-pasteable TOML block for `~/.config/nevoflux/canvas-tools/`, (d) a button to open `nevoflux://settings`, (e) a Retry button that re-checks. See the "Missing-tool UX pattern" in the Canvas Tools section for a reusable template
24. **Share password is shown once only** — do not re-request it. Display immediately, offer copy-with-auto-clear, warn the user to save it
25. **Share URL domain** is `share.nevoflux.app` (not `.com`). Recipients open `nevoflux://import/{share_id}` which triggers the import flow in this canvas page
26. **Share import requires both URL and password** — no "password recovery". Wrong password fails with decryption error
27. **Rich text editors** (Google Docs, Notion, ProseMirror, Lexical): `type`/`fill` often fail because target is `contenteditable`. Use `paste` to insert at caret or `fillRichText` to replace all content
28. **`navigate` without `new_tab`** reuses the current tab or falls back to any existing web tab. If you always want a fresh tab, pass `{ url, new_tab: true }`
29. **`uploadFile` `fileUrl`** is typically obtained from `cache_file` (returns a `file_path` usable as URL). Pass the file URL, not the raw bytes
30. **`activateTab` vs `navigate`**: use `activateTab({ tab_id })` to switch focus to an already-open tab, `navigate` to change the URL. Combine with `query_tabs` to find the target tab id first
