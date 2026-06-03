<!--
   - This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/.
   -->

<p align="center">
  <img src="./docs/assets/nevoflux-logo.svg" width="120px" alt="Nevoflux Logo">
</p>

<h1 align="center">Nevoflux</h1>

<p align="center">
  <em>"Don't Just Browse. Command."</em>
</p>

<p align="center">
  <strong>The AI-native browser that acts as your agentic companion.</strong>
</p>

<p align="center">
  <a href="https://github.com/dorisgyl/nevoflux/actions/workflows/finalize-release.yml">
    <img src="https://github.com/dorisgyl/nevoflux/actions/workflows/finalize-release.yml/badge.svg" alt="Release Status">
  </a>
  <a href="https://github.com/dorisgyl/nevoflux/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MPL--2.0-blue.svg" alt="License">
  </a>
  <a href="https://github.com/dorisgyl/nevoflux/stargazers">
    <img src="https://img.shields.io/github/stars/dorisgyl/nevoflux?style=social" alt="Stars">
  </a>
</p>

<p align="center">
  <a href="https://discord.gg/u25Q5cNABg">
    <img src="https://img.shields.io/discord/1477910512430878741?logo=discord&logoColor=white&label=Discord&color=5865F2" alt="Discord">
  </a>
  <a href="https://x.com/NevoFlux">
    <img src="https://img.shields.io/badge/Follow-%40NevoFlux-black?logo=x&logoColor=white" alt="X (Twitter)">
  </a>
</p>

<p align="center">
  <a href="#highlights">Highlights</a> &middot;
  <a href="#three-execution-modes">Modes</a> &middot;
  <a href="#getting-started">Getting Started</a> &middot;
  <a href="#architecture">Architecture</a> &middot;
  <a href="#contributing">Contributing</a>
</p>

<p align="center">Currently built on Zen Browser <code>1.19.6</code> and Firefox <code>149.0</code></p>

---

## What is Nevoflux?

**Nevoflux** is not just another browser. It's your personal AI companion that lives in your browser — it has its own identity, its own soul, and it learns from every interaction with you.

Built on [Zen Browser](https://zen-browser.app/) (Firefox/Gecko), Nevoflux bridges the gap between you, AI, and the internet. It browses with you, thinks with you, and acts for you — from simple conversations to autonomous web navigation and full computer control.

Under the hood: a Rust-native agent daemon with 16+ LLM providers, 80+ browser automation APIs, cross-platform computer control, MCP integration, an encrypted cross-session memory system, a GBrain-powered knowledge base (your second brain), and integration with external coding agents and personal AI assistants.

---

## Highlights

- 🧬 **Identity & Soul** — Your companion has its own personality, values, and behavioral patterns that evolve over time
- 🧠 **Memory & Learning** — Remembers across sessions, learns your preferences, adapts to how you work — with encrypted storage
- 🎛️ **Progressive Autonomy** — Three modes (Chat, Browser, Agent) — you decide how much control to hand over
- ⚡ **Build: Micro Apps (Canvas)** — Generate fully functional mini-apps on the fly — not just rendered artifacts, but living apps with full agent capabilities. Exportable to 10+ formats (HTML, PNG, PDF, DOCX, PPTX, XLSX, SVG, ZIP). Every other AI just shows you text — Nevoflux creates tools you can actually use.
- 📚 **Remember: Second Brain (GBrain)** — A personal knowledge base you grow on purpose — capture pages, notes, and research, then recall and synthesize across everything you've saved. Distinct from the agent's automatic memory: this is *your* curated brain, stored as local markdown pages and shareable via encrypted, zero-knowledge links. Canvas Micro Apps help you **build** tools; GBrain helps you **remember** what matters.
- 🧩 **WASM Skills** — Extensible skill system powered by WebAssembly — sandboxed, pluggable, and progressively loaded
- 🖥️ **Browser + Computer Control** — 80+ browser APIs and cross-platform desktop control (screenshot, mouse, keyboard)
- 🤖 **Multi-LLM Freedom** — 16+ providers: Anthropic, OpenAI, Qwen, DeepSeek, Gemini, Ollama, and more — your choice
- 🔧 **Coding Agent Delegation** — Seamlessly delegate development tasks to Claude Code, Gemini CLI, or Kimi Agent — they work as sub-agents within your browser

---

## Three Execution Modes

Nevoflux uses progressive capability unlocking — each mode builds on the previous one, giving you control over how much autonomy your companion has.

**Chat** — Talk to your companion. Ask questions, search the web, let it read what's on your screen.

**Browser** — Let it take the wheel. It clicks, types, navigates, and fills forms for you.

**Agent** — Full autonomy. File operations, shell commands, computer control, sub-agents — your companion handles complex multi-step tasks end-to-end.

| Mode        | Capabilities                                                          |
| ----------- | --------------------------------------------------------------------- |
| **Chat**    | LLM reasoning, memory, web search, read-only browser access           |
| **Browser** | Chat + click, type, navigate, fill forms, interact with pages         |
| **Agent**   | Browser + file ops, shell commands, computer control, MCP, sub-agents |

---

## Supported LLM Providers & Agents

### LLM Providers

| Provider         | Type  | Notes                     |
| ---------------- | ----- | ------------------------- |
| Anthropic Claude | API   | Default (claude-sonnet-4) |
| OpenAI           | API   | GPT-4o series             |
| Google Gemini    | API   | Up to 1M context window   |
| Qwen (通义千问)  | API   | DashScope integration     |
| DeepSeek         | API   |                           |
| Ollama           | Local | Run local models          |
| Groq             | API   |                           |
| Mistral          | API   |                           |
| XAi              | API   |                           |
| Cohere           | API   |                           |
| Perplexity       | API   |                           |
| Together         | API   |                           |
| OpenRouter       | API   |                           |

### Agents

| Agent            | Type     | Notes                             |
| ---------------- | -------- | --------------------------------- |
| Claude Code      | Coding   | Subprocess integration            |
| Gemini CLI       | Coding   | Subprocess integration            |
| Kimi Agent       | Coding   | Subprocess integration            |
| OpenClaw         | Personal | Personal AI assistant integration |

Configure your preferred provider in `~/.config/nevoflux/config.toml`:

```toml
[llm]
active_provider = "anthropic"
default_model = "claude-sonnet-4-20250514"
```

---

## Getting Started

Nevoflux consists of two components: **the browser** (this repository) and the **Rust native agent** ([nevoflux-agent](https://github.com/dorisgyl/nevoflux-agent)). Both are needed for full functionality.

### Browser

```bash
# Prerequisites: Node.js 22+, npm, Python 3

git clone https://github.com/dorisgyl/nevoflux.git
cd nevoflux

npm install
npm run download
npm run bootstrap
npm run import
npm run build
npm run start
```

### Native Agent

```bash
# Prerequisites: Rust 1.75+

git clone https://github.com/dorisgyl/nevoflux-agent.git
cd nevoflux-agent

cargo build --release
```

Then register the native messaging host:

```bash
# From the nevoflux (browser) repo
./scripts/setup-native-host.sh
```

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────────────┐
│                          Entry Layer                                      │
│                                                                           │
│  Browser Extension         Claude Code (MCP)         CLI                  │
│  (Native Messaging)        (stdio JSON-RPC)          (--daemon/--mcp)     │
│         │                        │                         │              │
└─────────┼────────────────────────┼─────────────────────────┼──────────────┘
          │                        │                         │
          └────────────────────────┼─────────────────────────┘
                                   │
                                   ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                        Daemon Layer (Rust)                                 │
│                                                                           │
│  ┌──────────────┐  ┌───────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │ Agent Engine  │  │ Context       │  │ Session     │  │ Permission  │  │
│  │ (agentic loop)│  │ Builder       │  │ Manager     │  │ Enforcer    │  │
│  └──────┬────────┘  └───────────────┘  └─────────────┘  └─────────────┘  │
│         │                                                                 │
│  ┌──────▼──────────────────────────────────────────────────────────────┐  │
│  │                      Backend Services                                │  │
│  │                                                                      │  │
│  │  LLM Providers    │ Browser Tools  │ Computer  │ WASM  │ MCP       │  │
│  │  (16+ services,   │ (80+ APIs)     │ Control   │ Skills│ Server +  │  │
│  │   local, agents)  │                │           │       │ Client    │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                           │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │           Memory & Learning (encrypted, cross-session)               │  │
│  │  Knowledge Graph │ Conversation History │ User Preferences │ Skills  │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                           │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │               SQLite Storage (WAL mode, encrypted)                   │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────┘

         Build ▼                            Remember ▼
  ┌─────────────────────┐          ┌──────────────────────────┐
  │  Canvas Micro Apps   │          │  GBrain                   │
  │  Create tools,       │          │  Second brain —           │
  │  export to 10+       │          │  capture, recall,         │
  │  formats             │          │  synthesize               │
  └─────────────────────┘          └──────────────────────────┘

         Code ▼
  ┌────────────┐ ┌─────────────┐ ┌─────────────┐
  │ Claude Code │ │ Gemini CLI  │ │ Kimi Agent  │
  │ (coding)    │ │ (coding)    │ │ (coding)    │
  └────────────┘ └─────────────┘ └─────────────┘
                Coding Agents
```

### Browser-Side Components

```
nevoflux (browser repo)
├── src/nevoflux/
│   ├── extensions/nevoflux-agent/     # WebExtension (sidebar UI, background.js)
│   │   ├── dioxus-ui/chat-sidebar/    # Dioxus/WASM sidebar application
│   │   ├── background/               # Extension background script
│   │   └── content/                   # Content scripts for browser tools
│   ├── engine-overlays/               # Browser chrome modifications
│   │   ├── browser/actors/            # NevofluxChild/Parent actors
│   │   ├── browser/components/        # nevoflux:// pages (canvas, settings)
│   │   └── browser/modules/           # Bridge router, content store
│   ├── patches/                       # Zen Browser patches
│   └── overlays/                      # New files for src/zen/
```

### Crate Structure (Native Agent)

The native agent is organized as a Rust workspace with 10 crates:

```
nevoflux-agent/
├── nevoflux-daemon        # Core daemon (agent engine, session, memory, permissions, WASM runtime)
├── nevoflux-bridge        # Proxy bridges (Native Messaging, MCP)
├── nevoflux-protocol      # Message types and serialization (MessagePack + JSON)
├── nevoflux-llm           # LLM provider abstraction (via rig-core)
├── nevoflux-storage       # SQLite persistence (sessions, messages, memory, permissions)
├── nevoflux-mcp           # MCP server + client integration
├── nevoflux-computer      # Cross-platform computer control (X11, macOS, Windows)
├── nevoflux-skills        # WASM skill loading and management
├── nevoflux-builtin-wasm  # Built-in WASM agent module
└── nevoflux-testing       # Test infrastructure (mocks, builders, helpers)
```

---

## Canvas (Micro Apps)

Nevoflux's Canvas system lets you generate interactive artifacts — HTML apps, React components, SVG graphics, Mermaid diagrams, multi-file projects, and more — directly in the chat.

Canvas artifacts are living applications with access to the NevofluxSDK, enabling them to call browser tools, interact with the agent, and access storage.

### Export Formats

| Format     | Availability                    |
| ---------- | ------------------------------- |
| Source     | Always                          |
| HTML       | Always                          |
| PNG        | Always                          |
| PDF        | Always                          |
| DOCX       | Always                          |
| SVG        | SVG / Mermaid artifacts         |
| Markdown   | HTML artifacts                  |
| PPTX       | Slides artifacts                |
| XLSX       | Artifacts containing tables     |
| ZIP        | Project (multi-file) artifacts  |

---

## Knowledge Base (GBrain)

Beyond the agent's automatic, behind-the-scenes memory, Nevoflux gives you a **second brain** — a personal knowledge base you curate on purpose, persisted by the **GBrain** backend. Open it at `nevoflux://brain` (enable it first under `nevoflux://settings/knowledge-base`).

Each entry is a markdown page with a stable slug, a `compiled_truth` body (what you currently believe), and a `timeline` of how that understanding evolved. Four operations cover the lifecycle:

| Operation      | What it does                                                                          |
| -------------- | ------------------------------------------------------------------------------------- |
| **Recall**     | Look something up — "what do I know about X", "catch me up", "what's notable lately"   |
| **Capture**    | Save or ingest — a note, a page, a PDF, a meeting — into a filed, back-linked page     |
| **Synthesize** | Connect the dots — concept maps, "who knows about X", how a topic has trended          |
| **Maintain**   | Health checks, sync, and recovery — undo a delete, surface inconsistencies             |

Pages live locally and are bilingual-friendly (English / 中文). Share a single page or your whole brain via **encrypted, zero-knowledge links** — the decryption key stays in the URL fragment and never reaches a server — and import knowledge others share with you.

---

## MCP Integration

Nevoflux plays a dual role in the MCP ecosystem:

- **As MCP Server** — Exposes 23+ tools (browser, computer, agent) to external clients like Claude Code
- **As MCP Client** — Connects to external MCP servers (filesystem, GitHub, and more)

```toml
# ~/.config/nevoflux/mcp-servers.toml

[servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/user"]

[servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "..." }
```

---

## Security & Permissions

Nevoflux uses a **default-deny** permission model. Every sensitive action requires explicit approval.

- **Human-in-the-loop** — Security approval, quality control, decision branching, and error intervention
- **Sensitive path blocklist** — `.ssh/`, `.gnupg/`, `.aws/credentials`, and other critical paths are always blocked
- **Permission scopes** — Grant access once, for the session, or permanently
- **Encrypted storage** — Memory and knowledge data encrypted with AES-256-GCM
- **SDK sandboxing** — Canvas artifacts run in sandboxed iframes; the NevofluxSDK is only injected into `nevoflux://` pages, never external websites

---

## Configuration

```toml
# ~/.config/nevoflux/config.toml

[daemon]
port_range_start = 19500
idle_timeout_secs = 1800          # 30 min idle shutdown

[llm]
active_provider = "anthropic"
default_model = "claude-sonnet-4-20250514"
max_tokens = 4096
temperature = 0.7

[session]
max_sessions = 500
inactive_cleanup_days = 90

[storage]
max_size_mb = 1024
wal_mode = true
```

API keys are resolved in order: environment variables → system keychain → config file.

---

## FAQ

<details open>
<summary><strong>Q: How do I open the AI sidebar?</strong></summary>

> Press `Ctrl+Shift+A` (Windows/Linux) or `Cmd+Shift+A` (macOS) to toggle the AI sidebar open and closed.
</details>

<details open>
<summary><strong>Q: How do I configure LLM providers and agents?</strong></summary>

> Type `nevoflux://settings` in the address bar and navigate to **AI Models**. You'll see two sections: **LLM Providers** (cloud APIs and local models) and **Agents** (coding agents and personal assistants). Click any card to configure its API key and settings.
</details>

<details open>
<summary><strong>Q: How do I connect Claude Code as a coding agent?</strong></summary>

> First install the Claude Code ACP adapter globally:
>
> ```bash
> npm install -g @agentclientprotocol/claude-agent-acp
> ```
>
> Then go to `nevoflux://settings` → AI Models → click the **Claude Code** card and configure it.
</details>

---

## Contributing

Nevoflux is open source and welcomes contributions.

- **Discord** — [Join our community](https://discord.gg/u25Q5cNABg)
- **Bug Reports** — [GitHub Issues](https://github.com/dorisgyl/nevoflux/issues)
- **Feature Requests** — [GitHub Discussions](https://github.com/dorisgyl/nevoflux/discussions)
- **Code Contributions** — See [CONTRIBUTING.md](./CONTRIBUTING.md)

### Development Workflow

Nevoflux uses a **patch-based system** for customizing Zen Browser. The `engine/` directory is regenerated from upstream + patches — never edit it directly.

```bash
# Development cycle
npm run import # Apply patches/overlays
# ... make changes in src/zen/ ...
npm run build:ui # Quick UI rebuild
npm run start    # Test in browser

# Before committing
./scripts/export-nevoflux-patches.sh # Export changes as patches
./scripts/revert-zen-changes.sh      # Revert src/zen/
git add src/nevoflux/patches/
git commit -m "patch(feature): description"
```

### Build Commands

| Command              | Description                     |
| -------------------- | ------------------------------- |
| `npm run import`     | Apply patches and overlays      |
| `npm run build`      | Full browser build              |
| `npm run build:ui`   | UI-only rebuild (faster)        |
| `npm run start`      | Launch the browser              |
| `npm run test`       | Run tests                       |
| `npm run lint`       | Run ESLint/Prettier             |

---

## Acknowledgments

- [Zen Browser](https://zen-browser.app/) — The productivity-focused browser that serves as our foundation
- [Firefox](https://www.mozilla.org/firefox/) — The open-source Gecko engine
- [OpenClaw](https://openclaw.ai/) — Personal AI assistant framework
- [Wasmtime](https://wasmtime.dev/) — WebAssembly runtime
- [Model Context Protocol](https://modelcontextprotocol.io/) — AI-tool integration protocol

---

## License

Nevoflux is licensed under the [Mozilla Public License 2.0](./LICENSE).

---

<p align="center">
  <strong>Nevoflux</strong> — Don't Just Browse. Command.
</p>

---

## Maintainer

This project is developed and maintained by **YULIN GAN**.

- GitHub: https://github.com/dorisgyl
- Contact: doris_gyl@hotmail.com
