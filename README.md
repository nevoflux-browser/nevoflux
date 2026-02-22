<!--
   - This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/.
   -->

<p align="center">
  <img src="./docs/assets/nevoflux-logo.svg" width="120px" alt="nevoflux Logo">
</p>

<h1 align="center">nevoflux</h1>

<p align="center">
  <strong>AI-powered agentic browser built on Zen Browser for autonomous web navigation and intelligent task automation</strong>
</p>

<p align="center">
  <a href="https://github.com/user/nevoflux/releases">
    <img src="https://img.shields.io/github/downloads/user/nevoflux/total.svg" alt="Downloads">
  </a>
  <a href="https://github.com/user/nevoflux/actions/workflows/build.yml">
    <img src="https://github.com/user/nevoflux/actions/workflows/build.yml/badge.svg" alt="Build Status">
  </a>
  <a href="https://github.com/user/nevoflux/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MPL--2.0-blue.svg" alt="License">
  </a>
  <a href="https://github.com/user/nevoflux/stargazers">
    <img src="https://img.shields.io/github/stars/user/nevoflux?style=social" alt="Stars">
  </a>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#plugins">Plugins</a> •
  <a href="#documentation">Documentation</a> •
  <a href="#contributing">Contributing</a>
</p>

---

## What is nevoflux?

**nevoflux** is an AI-native agentic browser that combines the power of [Zen Browser](https://zen-browser.app/) with autonomous AI agents. It enables intelligent task automation, autonomous web navigation, and seamless human-AI collaboration — all within a beautiful, productivity-focused browser.

```
┌─────────────────────────────────────────────────────┐
│                    nevoflux                         │
│                                                     │
│   User Intent  ──►  Agent Plugins  ──►  Results    │
│                         │                           │
│                   Human in the Loop                 │
│                                                     │
└─────────────────────────────────────────────────────┘
```

### Why nevoflux?

- 🚀 **Built on Zen Browser** — Inherit all productivity features from Zen, powered by Firefox/Gecko
- 🤖 **Autonomous Agents** — AI agents that can browse, click, type, and complete tasks for you
- 🔌 **Extensible Plugin System** — Build custom agents with WASM plugins
- 🔓 **Model Freedom** — Choose any LLM provider (OpenAI, Anthropic, local models, etc.)
- 👤 **Human in the Loop** — Stay in control at critical decision points
- 🏢 **Enterprise Ready** — Zero-trust architecture for corporate deployments

---

## Features

### 🤖 Agentic Capabilities

- **Autonomous Web Navigation** — AI agents that understand web pages and interact intelligently
- **Intelligent Task Automation** — Automate repetitive tasks with natural language instructions
- **WASM Plugin System** — Build custom agents using Rust, Go, TypeScript, or any WASM-compatible language
- **Capability-based Security** — Plugins have no inherent privileges; all capabilities require browser authorization
- **Dual Control Loop** — Plugins control agent logic while the browser acts as a gatekeeper

### 🔗 Protocol Support

- **MCP (Model Context Protocol)** — Connect to external data sources and tools
- **A2A (Agent-to-Agent)** — Enable collaboration between multiple agents

### 👁️ Human in the Loop

Four types of human intervention, all supported:

| Type | Description |
|------|-------------|
| **Security Approval** | Confirm sensitive operations |
| **Quality Control** | Review AI-generated content before proceeding |
| **Decision Branch** | Choose between multiple options |
| **Error Intervention** | Correct agent when it's stuck or off-track |

### 🌐 Zen Browser Foundation

Built on [Zen Browser](https://zen-browser.app/), which is based on Firefox:

- 🎨 Beautiful, modern UI designed for productivity
- ⚡ Fast and efficient, powered by Gecko engine
- 🔒 Privacy-focused with Firefox's security features
- 🧩 Full Firefox extension compatibility
- 💻 Cross-platform (Windows, macOS, Linux)

---

## Installation

### Download

> 🚧 **Coming Soon** — nevoflux is currently in active development.

<!--
| Platform | Download |
|----------|----------|
| Windows  | [nevoflux-win-x64.exe](#) |
| macOS    | [nevoflux-macos-arm64.dmg](#) |
| Linux    | [nevoflux-linux-x64.AppImage](#) |
-->

### Build from Source

```bash
# Clone the repository
git clone https://github.com/user/nevoflux.git
cd nevoflux

# Install dependencies (requires Node.js 18+, npm, Python 3)
npm install

# Download and bootstrap Zen Browser dependencies
npm run download
npm run bootstrap

# Import patches and overlays to src/zen/
npm run import

# Build the browser (this takes a while on first run)
npm run build

# Run the browser
npm run start
```

---

## Plugins

nevoflux's power comes from its extensible plugin system. Plugins are WASM components that can:

- Control browser actions (navigate, click, type, read DOM)
- Call LLM APIs through a unified interface
- Access external resources via MCP
- Collaborate with other plugins via A2A

### Example Plugin

```rust
// A simple agent that searches and summarizes
use nevoflux_sdk::prelude::*;

#[nevoflux_plugin]
fn run(ctx: &mut Context) -> Result<()> {
    // Read user intent
    let query = ctx.input("What would you like to research?")?;
    
    loop {
        // Request permission for next iteration
        ctx.request_iteration()?;
        
        // Observe the page
        let page_content = ctx.browser().get_page_content()?;
        
        // Think with LLM
        let action = ctx.llm().chat(&[
            Message::system("You are a research assistant..."),
            Message::user(&format!("Query: {}\nPage: {}", query, page_content)),
        ])?;
        
        // Execute action
        match action.action_type {
            ActionType::Search(q) => ctx.browser().navigate(&format!("https://google.com/search?q={}", q))?,
            ActionType::Click(selector) => ctx.browser().click(&selector)?,
            ActionType::Done(summary) => {
                ctx.output(&summary)?;
                break;
            }
        }
    }
    
    Ok(())
}
```

### Plugin Development

```bash
# Install the nevoflux plugin CLI
cargo install nevoflux-cli

# Create a new plugin project
nevoflux new my-agent

# Build the plugin
cd my-agent
nevoflux build

# Install locally for testing
nevoflux install ./target/my-agent.wasm
```

📖 See the [Plugin Development Guide](./docs/plugins/README.md) for more details.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         nevoflux                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    Plugin Runtime                        │   │
│  │   ┌─────────┐  ┌─────────┐  ┌─────────┐                │   │
│  │   │Plugin A │  │Plugin B │  │Plugin C │  (WASM)        │   │
│  │   └────┬────┘  └────┬────┘  └────┬────┘                │   │
│  └────────┼────────────┼────────────┼──────────────────────┘   │
│           │            │            │                           │
│  ┌────────▼────────────▼────────────▼──────────────────────┐   │
│  │              Capability Interface (WIT)                  │   │
│  │   DOM │ LLM │ Network │ Files │ HITL │ MCP │ A2A       │   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                  │
│  ┌───────────────────────────┴──────────────────────────────┐  │
│  │                   Zen Browser Core                        │  │
│  │                  (Firefox / Gecko Engine)                 │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                   Zen Browser UI                          │  │
│  │    (Sidebar, Split View, Workspaces, Compact Mode)        │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Roadmap

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 0 | 🔨 In Progress | Foundation — Zen Browser integration, WASM runtime setup |
| Phase 1 | ⏳ Planned | MVP — Complete agent loop, LLM integration, demo plugin |
| Phase 2 | ⏳ Planned | Plugin ecosystem — Packaging, management UI, developer docs |
| Phase 3 | ⏳ Planned | Protocol integration — MCP client, A2A hub |
| Phase 4 | ⏳ Planned | Full browser — Enhanced Zen features with agentic capabilities |
| Phase 5 | ⏳ Planned | Enterprise — SSO, audit logs, policy management |

---

## Comparison

| Feature | nevoflux | Traditional Browsers | Other AI Browsers |
|---------|----------|---------------------|-------------------|
| Autonomous Agents | ✅ | ❌ | ⚠️ Limited |
| Custom Agent Plugins | ✅ WASM | ❌ | ❌ |
| Human in the Loop | ✅ 4 types | ❌ | ⚠️ Basic |
| Model Freedom | ✅ Any LLM | ❌ | ❌ Vendor-locked |
| MCP Support | ✅ | ❌ | ❌ |
| A2A Protocol | ✅ | ❌ | ❌ |
| Firefox Extensions | ✅ | ✅ Firefox only | ❌ |
| Privacy-focused | ✅ | ⚠️ Varies | ⚠️ Varies |
| Open Source | ✅ | ⚠️ Varies | ❌ Most |

---

## Documentation

- 📖 [User Guide](./docs/user-guide/README.md)
- 🔌 [Plugin Development](./docs/plugins/README.md)
- 🏗️ [Architecture](./docs/architecture/README.md)
- 🔧 [API Reference](./docs/api/README.md)
- 🏢 [Enterprise Deployment](./docs/enterprise/README.md)

---

## Contributing

nevoflux is an open-source project, and we welcome contributions from the community!

- 🐛 **Bug Reports** — [GitHub Issues](https://github.com/user/nevoflux/issues)
- 💡 **Feature Requests** — [GitHub Discussions](https://github.com/user/nevoflux/discussions)
- 🔧 **Code Contributions** — See [CONTRIBUTING.md](./CONTRIBUTING.md)

### Development Setup

```bash
# Prerequisites: Node.js 18+, npm, Python 3, Rust (for native agent)

# Clone and setup
git clone https://github.com/user/nevoflux.git
cd nevoflux
npm install
npm run download
npm run bootstrap

# Apply patches and overlays to src/zen/
npm run import

# Run the browser
npm run start

# Run tests
npm run test
```

### Development Workflow

nevoflux uses a **patch-based system** for customizing Zen Browser. The `src/zen/` directory contains the upstream Zen code, and customizations are stored as patches in `src/nevoflux/`.

```
┌─────────────────────────────────────────────────────────────┐
│  npm run import                                             │
│      ↓                                                      │
│  Develop & Test (src/zen/ has patches + overlays applied)   │
│      ↓                                                      │
│  git commit                                                 │
│      ↓                                                      │
│  [pre-commit] Auto-revert src/zen/                          │
│      ↓                                                      │
│  Commit succeeds                                            │
│      ↓                                                      │
│  [post-commit] Auto-restore src/zen/                        │
│      ↓                                                      │
│  Continue development                                       │
└─────────────────────────────────────────────────────────────┘
```

**Key points:**
- **Never commit changes directly to `src/zen/`** — all customizations go through patches
- Git hooks automatically handle `src/zen/` cleanup and restoration
- After first `npm run import`, you can develop and commit freely

### Modifying Zen Browser Code

If you need to modify Zen Browser code:

```bash
# 1. Make changes in src/zen/
# 2. Test your changes
# 3. Export as patches
./scripts/export-nevoflux-patches.sh

# 4. Revert src/zen/ (or just commit - hooks will auto-revert)
./scripts/revert-zen-changes.sh

# 5. Commit the patches
git add src/nevoflux/patches/
git commit -m "patch(feature): description"
```

### Build Commands

| Command | Description |
|---------|-------------|
| `npm run import` | Apply patches and overlays to src/zen/ |
| `npm run build` | Full browser build |
| `npm run build:ui` | UI-only rebuild (faster) |
| `npm run start` | Launch the browser |
| `npm run reload-ext` | Reload extension (packages + clears caches) |
| `npm run test` | Run tests |
| `npm run lint` | Run ESLint/Prettier |

---

## Acknowledgments

nevoflux is built on the shoulders of giants:

- [Zen Browser](https://zen-browser.app/) — The beautiful, productivity-focused browser that serves as our foundation
- [Firefox](https://www.mozilla.org/firefox/) — The open-source browser engine (Gecko)
- [Wasmtime](https://wasmtime.dev/) — WebAssembly runtime for plugin execution
- [Model Context Protocol](https://modelcontextprotocol.io/) — Protocol for AI-tool integration

---

## Community

- 💬 [Discord](https://discord.gg/nevoflux) (Coming Soon)
- 🐦 [Twitter](https://twitter.com/nevoflux) (Coming Soon)
- 📧 [Mailing List](mailto:dev@nevoflux.app) (Coming Soon)

---

## License

nevoflux is licensed under the [Mozilla Public License 2.0](./LICENSE).

---

<p align="center">
  <strong>nevoflux</strong> — AI-powered browsing for the autonomous future.
</p>

---

## Maintainer

This project is developed and maintained by **YULIN GAN**.

- Contact: doris_gyl@hotmail.com
