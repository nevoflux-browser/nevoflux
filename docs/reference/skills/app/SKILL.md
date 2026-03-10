---
name: app
description: Build interactive Canvas apps and visual outputs using NevofluxSDK. Supports HTML single-page apps, React components, multi-file projects (bundled with esbuild), SVG graphics, Mermaid diagrams, and Markdown documents — all with persistent storage, browser control, and AI agent interaction. Use this skill whenever the user wants to create any visual app, dashboard, widget, tool, game, calculator, chart, diagram, data visualization, interactive UI, or any output that should be rendered visually — even if they don't say "canvas" or "artifact" explicitly.
version: 1.1.0
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

| Type | `content_type` | When to Use |
|------|---------------|-------------|
| **HTML** | `"text/html"` | Default. Single-page apps, tools, dashboards, games |
| **React** | `"react"` | 3+ independent components or complex state management |
| **Project** | `"project"` | Multi-file apps needing module imports and bundling |
| **SVG** | `"image/svg+xml"` | Vector graphics, icons, illustrations |
| **Mermaid** | `"text/mermaid"` | Flowcharts, sequence diagrams, ER diagrams |
| **Markdown** | `"text/markdown"` | Formatted text, documentation, reports |

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
await NevofluxSDK.storage.get(key)           // Returns value or null
await NevofluxSDK.storage.set(key, value)    // Persist JSON-serializable value
await NevofluxSDK.storage.delete(key)        // Remove key
await NevofluxSDK.storage.query(prefix)      // List keys by prefix
```
Use app-specific key prefixes (e.g. `todo:items`, `dashboard:config`) to avoid collisions.

### Browser Tools (via callTool)
```javascript
await NevofluxSDK.callTool(action, params)
```
Action names use **snake_case**. Common actions:

| Action | Params | Purpose |
|--------|--------|---------|
| `navigate` | `{ url }` | Navigate browser tab |
| `get_markdown` | `{}` | Get page content as markdown |
| `screenshot` | `{}` | Capture page screenshot |
| `list_tabs` | `{}` | List all open tabs |
| `query_tabs` | `{ url?, title?, active? }` | Filter tabs |
| `get_elements` | `{}` | Get interactive page elements |
| `click` | `{ selector }` | Click element |
| `type` | `{ selector, text }` | Type into element |
| `web_search` | `{ query }` | Search the web |
| `web_fetch` | `{ url }` | Fetch URL content |
| `eval_js` | `{ code }` | Run JS in active browser tab |
| `ask_user` | `{ question, options? }` | Prompt user in sidebar |

Full action reference: load auxiliary file `callTool-actions.md`.

### Agent (bidirectional AI interaction)

`agent.chat()` sends a message to the AI agent and returns a Promise that resolves with the full response when the agent finishes. It supports streaming callbacks for real-time feedback.

**Simple (await final result):**
```javascript
const result = await NevofluxSDK.agent.chat("summarize this page");
// result = { text: "...", toolResults: [...], sessionId: "cs_..." }
```

**Streaming (real-time callbacks):**
```javascript
const result = await NevofluxSDK.agent.chat("analyze the data", {
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
  }
});
```

**Multi-turn conversation:**
```javascript
const r1 = await NevofluxSDK.agent.chat("find all images on this page");
const r2 = await NevofluxSDK.agent.chat("now download the first one", {
  sessionId: r1.sessionId  // continues same conversation
});
```

**Cancel a running request:**
```javascript
NevofluxSDK.agent.cancel(sessionId);
```

**System commands:**
```javascript
await NevofluxSDK.agent.sendCommand(command, params)
```
MCP tools are not directly callable. Use `agent.chat()` to ask the agent to invoke them.

### Sidebar
```javascript
await NevofluxSDK.sidebar.open()              // Open sidebar panel
await NevofluxSDK.sidebar.send(message)       // Send message to sidebar
await NevofluxSDK.sidebar.notify(type, data)  // Typed notification
```

### System
```javascript
await NevofluxSDK.system.getInfo()  // Get browser/system info
```

## HTML Template

```html
<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>My App</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: system-ui, sans-serif; padding: 20px; color: #1a1a1a; }
  .card { background: #fff; border: 1px solid #e5e5e5; border-radius: 8px; padding: 12px 16px; margin: 8px 0; }
  button { padding: 8px 16px; border: none; border-radius: 6px; background: #0066ff; color: #fff; cursor: pointer; }
  button:hover { background: #0052cc; }
  input { padding: 8px 12px; border: 1px solid #ddd; border-radius: 6px; outline: none; }
  input:focus { border-color: #0066ff; box-shadow: 0 0 0 2px rgba(0,102,255,0.15); }
</style>
</head><body>
  <h2>Notes</h2>
  <div style="display:flex;gap:8px;margin:12px 0">
    <input id="input" placeholder="New note..." style="flex:1">
    <button id="add">Add</button>
  </div>
  <div id="list"></div>
  <script>
    const KEY = "notes:items";
    async function load() {
      const items = (await NevofluxSDK.storage.get(KEY)) || [];
      render(items);
    }
    async function add() {
      const el = document.getElementById("input");
      if (!el.value.trim()) return;
      const items = (await NevofluxSDK.storage.get(KEY)) || [];
      items.push({ text: el.value.trim(), ts: Date.now() });
      await NevofluxSDK.storage.set(KEY, items);
      el.value = "";
      render(items);
    }
    function render(items) {
      document.getElementById("list").innerHTML =
        items.map(n => `<div class="card">${n.text}</div>`).join("");
    }
    document.getElementById("add").addEventListener("click", add);
    document.getElementById("input").addEventListener("keydown", e => {
      if (e.key === "Enter") add();
    });
    load();
  </script>
</body></html>
```

## React Template

Use when app has 3+ components or complex state. Canvas auto-injects React+Babel.
**content_type must be `"react"`.** Output ONLY JSX code — no HTML wrappers, no script tags, no React imports.

```jsx
function App() {
  const [tabs, setTabs] = React.useState([]);
  const [loading, setLoading] = React.useState(true);

  React.useEffect(() => {
    NevofluxSDK.callTool("list_tabs", {})
      .then(r => { if (r.success) setTabs(r.result?.tabs || []); })
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <div style={{ padding: 40, textAlign: "center" }}>Loading...</div>;

  return (
    <div style={{ fontFamily: "system-ui", padding: 20 }}>
      <h2>Tabs ({tabs.length})</h2>
      {tabs.map(tab => (
        <div key={tab.id}
             onClick={() => NevofluxSDK.callTool("navigate", { url: tab.url })}
             style={{ padding: 12, borderBottom: "1px solid #eee", cursor: "pointer" }}>
          <strong>{tab.title}</strong>
          <div style={{ fontSize: 12, color: "#888", marginTop: 4 }}>{tab.url}</div>
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
<html><head><meta charset="utf-8"><title>AI Assistant</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: system-ui, sans-serif; padding: 20px; max-width: 640px; }
  #output { white-space: pre-wrap; font-family: ui-monospace, monospace; background: #f5f5f5;
    padding: 16px; border-radius: 8px; min-height: 100px; margin: 12px 0; line-height: 1.5; }
  #status { font-size: 12px; color: #888; margin: 4px 0; }
  button { padding: 8px 16px; border: none; border-radius: 6px; background: #0066ff; color: #fff; cursor: pointer; }
  button:hover { background: #0052cc; }
  button:disabled { opacity: 0.5; cursor: default; }
</style>
</head><body>
  <h2>Page Analyzer</h2>
  <button id="analyze">Analyze Current Page</button>
  <div id="status">Ready</div>
  <div id="output"></div>
  <script>
    const btn = document.getElementById("analyze");
    const output = document.getElementById("output");
    const status = document.getElementById("status");

    btn.addEventListener("click", async () => {
      btn.disabled = true;
      output.textContent = "";
      status.textContent = "Thinking...";
      try {
        await NevofluxSDK.agent.chat(
          "Analyze the current page and summarize its key content in 3 bullet points",
          {
            onStream: (chunk) => { output.textContent += chunk.delta; },
            onState: (s) => { status.textContent = s.status; }
          }
        );
        status.textContent = "Done";
      } catch (err) {
        status.textContent = "Error: " + err.message;
      }
      btn.disabled = false;
    });
  </script>
</body></html>
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
7. Action names are **snake_case**: `get_markdown`, `list_tabs`, not camelCase
8. `callTool` returns `{ success, result, error }` — always check `success` first
9. MCP tools cannot be called directly — use `NevofluxSDK.agent.chat()` to ask the agent
10. `agent.chat()` resolves only after the agent finishes — use `onStream` callback for real-time output
11. Streaming callbacks (`onStream`, `onToolResult`, `onState`) are optional — omit for simple request-response
12. Use `sessionId` from a previous result to continue a multi-turn conversation
