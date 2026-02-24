# NevoFlux Dioxus UI

This directory contains the Dioxus WASM UI components for the NevoFlux Agent extension.

## Architecture

The UI is split into two main components:

### Chat Sidebar (`chat-sidebar/`)

The main chat interface displayed in the browser's sidebar panel. Features:

- Message input with markdown support
- Streaming response display
- Tab context awareness
- Native messaging integration

### Content Sidebar (`content-sidebar/`)

An overlay UI injected into web pages via Shadow DOM. Features:

- Default state: Displays current page URL
- Active state: Displays content from Chat Sidebar
- Style isolation via Shadow DOM
- Element highlighting support

### Shared Protocol (`shared-protocol/`)

Common types and message definitions used for communication between:

- Chat Sidebar ↔ Background Script
- Background Script ↔ Content Sidebar
- Both components ↔ Native Rust Agent

## Prerequisites

1. **Rust with WASM target**:

   ```bash
   rustup target add wasm32-unknown-unknown
   ```

2. **Trunk** (WASM bundler):

   ```bash
   cargo install trunk
   ```

3. **wasm-bindgen-cli**:
   ```bash
   cargo install wasm-bindgen-cli
   ```

## Building

### Using the build script (recommended):

```bash
./scripts/build-dioxus.sh
```

### Manual build:

```bash
# Build Chat Sidebar
cd chat-sidebar
trunk build --release

# Build Content Sidebar
cd ../content-sidebar
trunk build --release
```

### Copy to extension:

```bash
npm run copy:wasm
```

## Development

For hot-reloading during development:

```bash
# Terminal 1 - Chat Sidebar
cd chat-sidebar
trunk serve

# Terminal 2 - Content Sidebar
cd content-sidebar
trunk serve --port 8081
```

## Directory Structure

```
dioxus-ui/
├── Cargo.toml              # Workspace configuration
├── README.md               # This file
├── shared-protocol/        # Common types and protocol
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs          # ExtensionMessage enum and types
├── chat-sidebar/           # Main sidebar UI
│   ├── Cargo.toml
│   ├── Trunk.toml          # Trunk build config
│   ├── index.html          # Entry HTML
│   ├── assets/
│   │   └── chat-sidebar.css
│   └── src/
│       ├── lib.rs          # App entry point
│       ├── state.rs        # State types
│       ├── messaging.rs    # WebExtension messaging
│       ├── hooks.rs        # Custom Dioxus hooks
│       └── components/     # UI components
│           ├── mod.rs
│           ├── header.rs
│           ├── input_area.rs
│           ├── message_bubble.rs
│           ├── message_list.rs
│           └── status_bar.rs
├── content-sidebar/        # Page overlay UI
│   ├── Cargo.toml
│   ├── Trunk.toml
│   ├── index.html
│   ├── assets/
│   │   └── content-sidebar.css
│   └── src/
│       ├── lib.rs          # Shadow DOM injection
│       ├── state.rs        # State types
│       ├── messaging.rs    # Message handling
│       ├── shadow_host.rs  # Shadow DOM utilities
│       └── components/     # UI components
│           ├── mod.rs
│           ├── default_view.rs
│           ├── content_view.rs
│           ├── loading_view.rs
│           └── error_view.rs
└── dist/                   # Trunk build output
    ├── chat-sidebar/
    └── content-sidebar/
```

## Protocol Messages

See `shared-protocol/src/lib.rs` for the complete message definitions.

### Downstream (Chat → Content):

- `DisplayContent` - Send content to display
- `ClearContent` - Clear displayed content
- `HighlightElement` - Highlight page element
- `ClearHighlight` - Remove highlights

### Upstream (Content → Chat):

- `ContentUrlReport` - Report current URL
- `ContentElementClick` - Report element click
- `ContentSidebarReady` - Notify ready status

## Styling

Both components use a shared Zen theme with CSS variables:

- `--zen-primary` - Primary accent color
- `--zen-background` - Background color
- `--zen-surface` - Surface color
- `--zen-text` - Text color
- etc.

## License

Mozilla Public License 2.0
