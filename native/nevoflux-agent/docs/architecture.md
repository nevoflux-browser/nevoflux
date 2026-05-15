# NevoFlux Agent Architecture

## Overview

NevoFlux Agent is a modular system designed to bridge AI assistants with computer control capabilities. The architecture follows a hub-and-spoke pattern where a central daemon manages sessions, routes messages, and coordinates between multiple entry points (browser extensions, Claude Code, CLI) and execution backends (LLM providers, computer control, skills).

The system is built as a Rust workspace with clearly separated concerns:
- **Entry Points**: Multiple ways to interact with the agent (browser extension, MCP server, CLI)
- **Core Daemon**: Central orchestration handling sessions, routing, and agent execution
- **Backends**: Pluggable providers for LLM inference, computer control, and skill execution
- **Storage**: Persistent state with SQLite and vector search capabilities

## Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              ENTRY POINTS                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────┐  │
│  │  Browser Extension  │    │     Claude Code     │    │       CLI       │  │
│  │    (Chrome/FF)      │    │                     │    │   (nevoflux)    │  │
│  └──────────┬──────────┘    └──────────┬──────────┘    └────────┬────────┘  │
│             │                          │                        │           │
│             │ Native                   │ stdio                  │           │
│             │ Messaging                │                        │           │
│             ▼                          ▼                        │           │
│  ┌──────────────────────┐   ┌──────────────────────┐            │           │
│  │   Native Messaging   │   │     MCP Server       │            │           │
│  │       Proxy          │   │   (--mcp mode)       │            │           │
│  │  (--native mode)     │   │                      │            │           │
│  └──────────┬───────────┘   └──────────┬───────────┘            │           │
│             │                          │                        │           │
│             │ ZeroMQ                   │ ZeroMQ                 │           │
│             │                          │                        │           │
└─────────────┼──────────────────────────┼────────────────────────┼───────────┘
              │                          │                        │
              └──────────────────────────┼────────────────────────┘
                                         │
                                         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                            DAEMON SERVER                                    │
│                          (--daemon mode)                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                         ZeroMQ ROUTER                               │    │
│  │                    (Message Reception Layer)                        │    │
│  └────────────────────────────────┬────────────────────────────────────┘    │
│                                   │                                         │
│                                   ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                        Session Manager                              │    │
│  │         (Session lifecycle, authentication, state tracking)         │    │
│  └────────────────────────────────┬────────────────────────────────────┘    │
│                                   │                                         │
│                                   ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                           Router                                    │    │
│  │              (Message routing, request dispatching)                 │    │
│  └────────────────────────────────┬────────────────────────────────────┘    │
│                                   │                                         │
│                                   ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                        Agent Runner                                 │    │
│  │            (Agent execution, tool calling, streaming)               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
              │                          │                        │
              ▼                          ▼                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              BACKENDS                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────────────┐  │
│  │     Storage     │    │  LLM Providers  │    │    Computer Control     │  │
│  │    (SQLite)     │    │                 │    │                         │  │
│  │                 │    │  ┌───────────┐  │    │  ┌───────────────────┐  │  │
│  │  ┌───────────┐  │    │  │  Anthropic│  │    │  │  Screen Capture   │  │  │
│  │  │ Sessions  │  │    │  ├───────────┤  │    │  ├───────────────────┤  │  │
│  │  ├───────────┤  │    │  │  OpenAI   │  │    │  │  Mouse/Keyboard   │  │  │
│  │  │ Messages  │  │    │  ├───────────┤  │    │  ├───────────────────┤  │  │
│  │  ├───────────┤  │    │  │   Qwen    │  │    │  │  Window Manager   │  │  │
│  │  │  Skills   │  │    │  ├───────────┤  │    │  └───────────────────┘  │  │
│  │  ├───────────┤  │    │  │  Ollama   │  │    │                         │  │
│  │  │  Config   │  │    │  └───────────┘  │    └─────────────────────────┘  │
│  │  ├───────────┤  │    │                 │                                 │
│  │  │  Vectors  │  │    └─────────────────┘    ┌─────────────────────────┐  │
│  │  └───────────┘  │                           │        Skills           │  │
│  │                 │                           │                         │  │
│  └─────────────────┘                           │  ┌───────────────────┐  │  │
│                                                │  │   WASM Runtime    │  │  │
│                                                │  ├───────────────────┤  │  │
│                                                │  │  Built-in Skills  │  │  │
│                                                │  ├───────────────────┤  │  │
│                                                │  │  External Skills  │  │  │
│                                                │  └───────────────────┘  │  │
│                                                │                         │  │
│                                                └─────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Workspace Crates

| Crate | Path | Description |
|-------|------|-------------|
| `nevoflux` | `crates/cli/` | CLI binary with multiple operating modes (`--mcp`, `--daemon`, `--native`). Entry point for all interactions. |
| `nevoflux-daemon` | `crates/daemon/` | Core daemon server implementing session management, message routing, and agent execution. Runs as a long-lived background process. |
| `nevoflux-bridge` | `crates/bridge/` | Bridge implementations for external communication. Includes Native Messaging proxy for browser extensions and ZeroMQ client utilities. |
| `nevoflux-protocol` | `crates/protocol/` | Shared message types, request/response definitions, and serialization. Defines the contract between all components. |
| `nevoflux-storage` | `crates/storage/` | SQLite-based persistent storage with vector search capabilities. Manages sessions, messages, skills, and configuration. |
| `nevoflux-computer` | `crates/computer/` | Computer control capabilities including screen capture, mouse/keyboard input simulation, and window management. |
| `nevoflux-mcp` | `crates/mcp/` | Model Context Protocol server implementation. Exposes agent capabilities as MCP tools for integration with Claude Code and other MCP clients. |
| `nevoflux-llm` | `crates/llm/` | LLM provider abstraction layer. Supports multiple backends (Anthropic, OpenAI, Qwen, Ollama) with streaming and tool use. |
| `nevoflux-skills` | `crates/skills/` | Skill loading and management system. Handles discovery, validation, and execution of WASM-based skills. |
| `nevoflux-builtin-wasm` | `crates/builtin-wasm/` | Built-in WASM skills compiled into the binary. Provides core functionality available out-of-the-box. |
| `nevoflux-testing` | `crates/testing/` | Test utilities and fixtures shared across crates. Includes mock providers, test databases, and assertion helpers. |

## Communication Protocols

### Native Messaging Flow (Browser Extension)

The browser extension communicates with the daemon through a Native Messaging proxy:

```
Browser Extension
       │
       │ (1) Native Messaging Protocol
       │     - Length-prefixed JSON messages
       │     - stdin/stdout communication
       ▼
Native Messaging Proxy (nevoflux-agent)
       │
       │ (2) Message Translation
       │     - Parse browser message format
       │     - Convert to internal protocol
       │
       │ (3) ZeroMQ DEALER socket
       │     - Connect to daemon's ROUTER
       │     - Async request/response
       ▼
Daemon Server (nevoflux-agent --daemon)
       │
       │ (4) Response routing
       │     - ROUTER tracks client identity
       │     - Responses sent to correct client
       ▼
Native Messaging Proxy
       │
       │ (5) Response translation
       │     - Convert to Native Messaging format
       ▼
Browser Extension
```

### MCP Flow (Claude Code)

Claude Code integrates through the Model Context Protocol:

```
Claude Code
       │
       │ (1) MCP Protocol over stdio
       │     - JSON-RPC 2.0 messages
       │     - Tool discovery and invocation
       ▼
MCP Server (nevoflux-agent --mcp)
       │
       │ (2) Tool Handler
       │     - Map MCP tool calls to agent actions
       │     - Handle parameter validation
       │
       │ (3) ZeroMQ DEALER socket
       │     - Connect to daemon's ROUTER
       │     - Forward requests
       ▼
Daemon Server (nevoflux-agent --daemon)
       │
       │ (4) Agent execution
       │     - Process tool request
       │     - Return results
       ▼
MCP Server
       │
       │ (5) Response formatting
       │     - Convert to MCP response format
       ▼
Claude Code
```

### Daemon Protocol (ZeroMQ ROUTER/DEALER)

The daemon uses ZeroMQ's ROUTER/DEALER pattern for multiplexed communication:

```
                    ┌─────────────────────────────────────┐
                    │         Daemon Server               │
                    │                                     │
                    │    ┌───────────────────────────┐    │
                    │    │      ZeroMQ ROUTER        │    │
                    │    │   (ipc:///tmp/nevoflux)   │    │
                    │    │                           │    │
┌───────────┐       │    │   ┌─────────────────┐     │    │
│  Client A │◄──────┼────┼──►│ Identity: A     │     │    │
│  (DEALER) │       │    │   └─────────────────┘     │    │
└───────────┘       │    │                           │    │
                    │    │   ┌─────────────────┐     │    │
┌───────────┐       │    │   │ Identity: B     │     │    │
│  Client B │◄──────┼────┼──►│                 │     │    │
│  (DEALER) │       │    │   └─────────────────┘     │    │
└───────────┘       │    │                           │    │
                    │    │   ┌─────────────────┐     │    │
┌───────────┐       │    │   │ Identity: C     │     │    │
│  Client C │◄──────┼────┼──►│                 │     │    │
│  (DEALER) │       │    │   └─────────────────┘     │    │
└───────────┘       │    │                           │    │
                    │    └───────────────────────────┘    │
                    │                                     │
                    └─────────────────────────────────────┘

Message Format:
┌────────────────┬────────────────┬─────────────────────┐
│ Client Identity│ Empty Frame    │ Message Payload     │
│ (set by ROUTER)│ (delimiter)    │ (Protocol Message)  │
└────────────────┴────────────────┴─────────────────────┘
```

**Key characteristics:**
- **ROUTER** socket automatically tracks client identity
- **DEALER** sockets provide async request/response semantics
- Multiple clients can connect concurrently
- Messages are routed to the correct client based on identity frames
- IPC transport (`ipc:///tmp/nevoflux.sock`) for local communication

## Data Flow

A complete request from user interaction to response follows these steps:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           USER INTERACTION                                  │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. USER INPUT                                                               │
│    User types a message in browser extension or Claude Code issues a        │
│    tool call. Input is captured by the entry point.                         │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. MESSAGE ENCODING                                                         │
│    Entry point (Native Messaging Proxy or MCP Server) encodes the           │
│    request into the internal protocol format using nevoflux-protocol.       │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. TRANSPORT                                                                │
│    Message is sent over ZeroMQ DEALER socket to the daemon's ROUTER.        │
│    The ROUTER assigns/tracks the client identity for response routing.      │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 4. SESSION MANAGEMENT                                                       │
│    Session Manager validates the session, creates new sessions if needed,   │
│    and retrieves conversation history from storage.                         │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 5. REQUEST ROUTING                                                          │
│    Router examines the request type and dispatches to the appropriate       │
│    handler (agent execution, skill invocation, computer control, etc.).     │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 6. AGENT EXECUTION                                                          │
│    Agent Runner processes the request:                                      │
│    a. Constructs prompt with conversation history and available tools       │
│    b. Sends request to LLM provider (via nevoflux-llm)                      │
│    c. Processes streaming response                                          │
│    d. Handles tool calls if the LLM requests them                           │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 7. TOOL EXECUTION (if requested by LLM)                                     │
│    - Computer control actions via nevoflux-computer                         │
│    - Skill execution via nevoflux-skills                                    │
│    - Results are fed back to the LLM for continued processing               │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 8. PERSISTENCE                                                              │
│    Messages (user input, assistant response, tool calls/results) are        │
│    persisted to SQLite via nevoflux-storage for conversation continuity.    │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 9. RESPONSE ENCODING                                                        │
│    Final response is encoded into protocol format and sent back through     │
│    the ZeroMQ ROUTER to the correct client.                                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 10. RESPONSE DELIVERY                                                       │
│     Entry point receives the response, translates to the appropriate        │
│     format (Native Messaging JSON or MCP response), and delivers to user.   │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           USER RECEIVES RESPONSE                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Streaming Responses

For long-running requests, the agent supports streaming:

1. Daemon sends incremental response frames as LLM generates tokens
2. Entry points forward chunks to clients in real-time
3. Final frame indicates completion
4. Client UI updates progressively

### Error Handling

Errors at any stage are captured and returned as error responses:

- Transport errors: Connection failures, timeouts
- Session errors: Invalid session, authentication failures
- Execution errors: LLM failures, tool execution errors
- The error response includes error code, message, and optional details
