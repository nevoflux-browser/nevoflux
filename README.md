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
  <a href="https://github.com/dorisgyl/nevoflux/actions/workflows/build.yml">
    <img src="https://github.com/dorisgyl/nevoflux/actions/workflows/build.yml/badge.svg" alt="Build Status">
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

---

## What is Nevoflux?

**Nevoflux** is not just another browser. It's your personal AI companion that lives in your browser — it has its own identity, its own soul, and it learns from every interaction with you.

Built on [Zen Browser](https://zen-browser.app/) (Firefox/Gecko), Nevoflux bridges the gap between you, AI, and the internet. It browses with you, thinks with you, and acts for you — from simple conversations to autonomous web navigation and full computer control.

Under the hood: a Rust-native agent daemon with 16+ LLM providers, 80+ browser automation APIs, cross-platform computer control, MCP integration, and an encrypted cross-session memory system.

```
User ──► Sidebar UI ──► Browser Extension ──► Native Messaging ──► Rust Agent Daemon
                                                                       ├── LLM (16+ providers)
                                                                       ├── Browser Control (80+ APIs)
                                                                       ├── Computer Control
                                                                       ├── MCP (Server + Client)
                                                                       ├── Memory & Learning
                                                                       └── WASM Skills
```

---

## Highlights

- 🧬 **Identity & Soul** — Your companion has its own personality, values, and behavioral patterns that evolve over time
- 🧠 **Memory & Learning** — Remembers across sessions, learns your preferences, adapts to how you work — with encrypted storage
- 🎛️ **Progressive Autonomy** — Three modes (Chat, Browser, Agent) — you decide how much control to hand over
- ⚡ **Micro Apps** — Generate fully functional mini-apps on the fly — not just rendered artifacts, but living apps with full agent capabilities (browser automation, native tools, MCP services). Think Claude Artifacts, but alive.
- 🧩 **WASM Skills** — Extensible skill system powered by WebAssembly — sandboxed, pluggable, and progressively loaded
- 🖥️ **Browser + Computer Control** — 80+ browser APIs and cross-platform desktop control (screenshot, mouse, keyboard)
- 🤖 **Multi-LLM Freedom** — 16+ providers: Anthropic, OpenAI, Qwen, DeepSeek, Gemini, Ollama, and more — your choice

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

## Supported LLM Providers

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
| Claude Code      | CLI   | Subprocess integration    |
| Gemini CLI       | CLI   | Subprocess integration    |

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
┌─────────────────────────────────────────────────────────────────────┐
│                       Entry Layer                                    │
│                                                                      │
│  Browser Extension        Claude Code (MCP)        CLI               │
│  (Native Messaging)       (stdio JSON-RPC)         (--daemon/--mcp)  │
│         │                       │                        │           │
└─────────┼───────────────────────┼────────────────────────┼───────────┘
          │                       │                        │
          └───────────────────────┼────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Daemon Layer (ZeroMQ ROUTER)                       │
│                                                                      │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────┐  ┌────────────┐ │
│  │ Agent Engine │  │ Context      │  │ Session    │  │ Permission │ │
│  │ (50 iter max)│  │ Builder      │  │ Manager    │  │ Enforcer   │ │
│  └──────┬───────┘  └──────────────┘  └────────────┘  └────────────┘ │
│         │                                                            │
│  ┌──────▼────────────────────────────────────────────────────────┐  │
│  │                    Backend Services                            │  │
│  │  LLM Providers │ Browser Tools │ Computer │ WASM │ MCP │ Memory│ │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │               SQLite Storage (WAL mode, encrypted)             │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### Crate Structure

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
| `npm run reload-ext` | Reload extension + clear caches |
| `npm run test`       | Run tests                       |
| `npm run lint`       | Run ESLint/Prettier             |

---

## Acknowledgments

- [Zen Browser](https://zen-browser.app/) — The productivity-focused browser that serves as our foundation
- [Firefox](https://www.mozilla.org/firefox/) — The open-source Gecko engine
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
