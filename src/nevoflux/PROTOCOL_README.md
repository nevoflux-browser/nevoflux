# Browser Agent Native Messaging Protocol V2.0 - Implementation

## Overview

This directory contains the complete implementation of the Browser Agent Native Messaging Protocol V2.0, as specified in `Browser Agent Protocol.md`. The protocol enables bidirectional communication between the browser extension (frontend) and the Rust native host (backend) using structured JSON envelopes.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Browser Extension                        │
│  ┌────────────────┐  ┌──────────────┐  ┌─────────────────┐ │
│  │ Protocol Client│  │   Dynamic    │  │  App Controller │ │
│  │  (protocol-    │◄─┤   Renderer   │◄─┤  (sidebar)      │ │
│  │   client.js)   │  │  (Lit)       │  │                 │ │
│  └────────┬───────┘  └──────────────┘  └─────────────────┘ │
└───────────┼─────────────────────────────────────────────────┘
            │ Native Messaging
            │ (4-byte length + JSON)
┌───────────▼─────────────────────────────────────────────────┐
│                    Rust Native Host                          │
│  ┌────────────┐  ┌───────────┐  ┌──────────┐  ┌──────────┐ │
│  │  Native    │  │  Session  │  │  Action  │  │  Stream  │ │
│  │  Messaging │◄─┤  Manager  │◄─┤  Router  │◄─┤  Manager │ │
│  │  Transport │  │           │  │          │  │          │ │
│  └────────────┘  └───────────┘  └──────────┘  └──────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## Directory Structure

```
src/nevoflux/
├── crates/
│   ├── nevoflux-common/
│   │   └── src/
│   │       ├── protocol.rs          # Protocol types and envelope
│   │       └── tests/
│   │           └── protocol_tests.rs # Protocol tests
│   └── nevoflux-agent/
│       ├── src/
│       │   ├── main.rs               # Entry point
│       │   ├── session.rs            # Session management
│       │   ├── action_router.rs      # Action ID routing
│       │   ├── stream_manager.rs     # Streaming output
│       │   └── native_messaging/
│       │       ├── mod.rs            # Module exports
│       │       ├── transport.rs      # 4-byte prefix transport
│       │       └── handler.rs        # Message handling
│       └── examples/
│           └── link_checker_scenario.rs # Complete example
└── extensions/
    └── nevoflux-agent/
        ├── sidebar/
        │   ├── sidebar.html          # Main HTML
        │   ├── app-controller.js     # Application logic
        │   └── components/
        │       └── dynamic-renderer.js # A2UI renderer
        └── utils/
            └── protocol-client.js    # Protocol client
```

## Protocol Implementation

### Core Components

#### 1. Envelope Structure

All messages use a unified envelope:

```json
{
  "ver": "2.0",
  "msg_id": "uuid-v4",
  "session_id": "uuid-v4",
  "type": "namespace.action",
  "payload": {},
  "timestamp": 1234567890
}
```

#### 2. Uplink Messages (Extension → Rust Agent)

- **input.chat**: User chat with multimodal support (text, files, context)
- **input.ui_event**: UI component interactions (buttons, forms)
- **input.command**: Direct commands (stop generation, etc.)
- **context.tab_update**: Browser tab state synchronization

#### 3. Downlink Messages (Rust Agent → Extension)

- **agent.stream.text**: Streaming text responses (markdown)
- **agent.ui.render**: Render A2UI components
- **agent.ui.update**: Update existing UI components
- **browser.control**: Browser automation commands

### Rust Backend

#### SessionManager

Manages active sessions, each bound to a browser tab:

```rust
let session_manager = SessionManager::new();
let session = session_manager.get_or_create("session-id".to_string()).await;

// Update session
session_manager.update("session-id", |session| {
    session.update_tab(tab_id, url, title);
}).await;
```

#### ActionRouter

Maps action_id from UI events to handler functions:

```rust
let router = ActionRouter::new();

// Register action handler
router.register("export_csv".to_string(), |session_id, action_id, form_data| async move {
    // Handle export
    Ok(serde_json::json!({"status": "success"}))
}).await;

// Handle action
let result = router.handle(session_id, action_id, form_data).await?;
```

#### StreamManager

Manages streaming text output:

```rust
let stream_manager = StreamManager::new(message_sender);

// Create stream
let rx = stream_manager.create_text_stream(session_id, stream_id).await?;

// Send text deltas
stream_manager.send_text(&stream_id, "Processing...".to_string()).await?;

// Finish stream
stream_manager.finish_stream(&stream_id).await?;
```

#### Native Messaging Transport

Handles 4-byte length prefix protocol:

```rust
// Reading messages
let reader = MessageReader::new();
let mut rx = reader.start_async();

while let Some(message) = rx.recv().await {
    // Process message
}

// Writing messages
let writer = AsyncMessageWriter::new();
writer.send(json_string)?;
```

### Frontend

#### Protocol Client

JavaScript client for protocol communication:

```javascript
import { protocolClient } from './utils/protocol-client.js';

// Connect
protocolClient.connect();

// Send chat
protocolClient.sendChat("Check current page links");

// Send UI event
protocolClient.sendUiEvent(viewId, actionId, formData);

// Handle streams
protocolClient.onStream('main', (data) => {
    console.log('Stream delta:', data.delta);
});

// Handle UI render
protocolClient.onUi('main', (data) => {
    if (data.action === 'render') {
        // Render UI component
    }
});
```

#### Dynamic Renderer

Lit component for rendering A2UI JSON:

```javascript
import './components/dynamic-renderer.js';

// Create renderer
const renderer = document.createElement('dynamic-renderer');
renderer.layout = a2uiJsonTree;
renderer.viewId = 'view_1';

// Listen for actions
renderer.addEventListener('action-triggered', (e) => {
    const { viewId, actionId, formData } = e.detail;
    protocolClient.sendUiEvent(viewId, actionId, formData);
});

// Update component
renderer.updateComponent('btn_1', { loading: true, label: 'Processing...' });
```

### A2UI Components

The protocol defines JSON-based UI components:

#### Container
```javascript
{
  "component": "Container",
  "props": { "layout": "flex-col", "gap": "md" },
  "children": [...]
}
```

#### Text
```javascript
{
  "component": "Text",
  "props": { "content": "Hello", "type": "h1", "color": "primary" }
}
```

#### Button
```javascript
{
  "component": "Button",
  "props": {
    "label": "Click Me",
    "action_id": "my_action",
    "variant": "primary"
  }
}
```

#### Input
```javascript
{
  "component": "Input",
  "props": { "name": "username", "type": "text", "placeholder": "Enter name" }
}
```

#### Table
```javascript
{
  "component": "Table",
  "props": {
    "headers": ["Name", "Age"],
    "rows": [["Alice", "30"], ["Bob", "25"]]
  }
}
```

#### Spinner
```javascript
{
  "component": "Spinner",
  "props": { "text": "Loading..." }
}
```

## Complete Example: Link Checker Scenario

The implementation includes a complete example demonstrating the protocol flow:

### Scenario Flow

1. **User Input**: "Check current page links"
2. **Agent Thinking**: Streams thinking process
3. **UI Render**: Displays table with dead links and export button
4. **User Action**: Clicks "Export CSV" button
5. **Agent Response**: Exports CSV and updates button state

### Running the Example

```bash
cd src/nevoflux/crates
cargo run --example link_checker_scenario
```

### Example Output

The example demonstrates:
- Creating and sending protocol envelopes
- Streaming text responses
- Building A2UI component trees
- Handling UI events
- Updating UI components

## Testing

### Rust Tests

```bash
cd src/nevoflux/crates
cargo test
```

Tests cover:
- Protocol envelope creation and serialization
- A2UI component builders
- Component ID assignment
- Message type serialization/deserialization
- Session management
- Action routing
- Stream management
- Native messaging transport

### Frontend Tests

Frontend components can be tested in the browser:

1. Load the extension in Firefox
2. Open the sidebar
3. Send test messages
4. Verify protocol communication in console

## Building

### Rust Agent

```bash
cd src/nevoflux/crates
cargo build --release
```

The binary will be at: `target/release/nevoflux-agent`

### Extension

The extension is already built as part of the NevoFlux browser build:

```bash
npm run build
```

## Configuration

### Native Messaging Manifest

The browser needs a native messaging manifest to locate the agent:

**Linux/Mac**: `~/.mozilla/native-messaging-hosts/com.nevoflux.agent.json`

```json
{
  "name": "com.nevoflux.agent",
  "description": "NevoFlux AI Agent",
  "path": "/path/to/nevoflux-agent",
  "type": "stdio",
  "allowed_extensions": ["nevoflux-agent@nevoflux.com"]
}
```

**Windows**: `HKEY_CURRENT_USER\Software\Mozilla\NativeMessagingHosts\com.nevoflux.agent`

## API Reference

### Rust API

#### nevoflux-common::protocol

```rust
// Envelope
pub struct Envelope<T> { ... }
impl<T> Envelope<T> {
    pub fn new(session_id: String, msg_type: String, payload: T) -> Self;
}

// Uplink messages
pub struct ChatInput { ... }
pub struct UiEventInput { ... }
pub struct CommandInput { ... }
pub struct TabUpdate { ... }

// Downlink messages
pub struct TextStream { ... }
pub struct UiRender { ... }
pub struct UiUpdate { ... }
pub struct BrowserControl { ... }

// A2UI components
pub struct A2UiComponent { ... }
impl A2UiComponent {
    pub fn new(component: impl Into<String>) -> Self;
    pub fn with_id(self, id: impl Into<String>) -> Self;
    pub fn with_prop(self, key: impl Into<String>, value: impl Serialize) -> Self;
    pub fn with_child(self, child: A2UiComponent) -> Self;
    pub fn assign_ids(&mut self, prefix: &str, counter: &mut usize);
}

// Helper functions
pub fn container(layout: &str) -> A2UiComponent;
pub fn text(content: impl Into<String>, text_type: &str) -> A2UiComponent;
pub fn button(label: impl Into<String>, action_id: impl Into<String>) -> A2UiComponent;
pub fn input(name: impl Into<String>, input_type: &str) -> A2UiComponent;
pub fn table(headers: Vec<String>, rows: Vec<Vec<String>>) -> A2UiComponent;
pub fn spinner(text: impl Into<String>) -> A2UiComponent;
```

### JavaScript API

#### ProtocolClient

```javascript
class ProtocolClient {
    connect(): void
    disconnect(): void

    // Uplink
    sendChat(text: string, files?: Array, contextRef?: Object): string
    sendUiEvent(viewId: string, actionId: string, formData?: Object): void
    sendCommand(cmd: string, args?: Object): void
    sendTabUpdate(tabId: number, url: string, title: string, status: string): void

    // Handlers
    onStream(id: string, handler: Function): void
    onUi(id: string, handler: Function): void
    onBrowserControl(handler: Function): void
    onConnectionChange(callback: Function): void

    // Utilities
    getSessionId(): string
    resetSession(): void
}
```

#### DynamicRenderer

```javascript
class DynamicRenderer extends LitElement {
    // Properties
    layout: Object    // A2UI component tree
    viewId: string    // View identifier

    // Methods
    updateComponent(componentId: string, props: Object): void

    // Events
    'action-triggered': CustomEvent<{viewId, actionId, formData}>
}
```

## Performance Considerations

1. **Streaming**: All text output uses streaming for responsive UX
2. **Component Updates**: Use `agent.ui.update` for partial updates instead of re-rendering entire trees
3. **Action Routing**: Direct action_id mapping avoids LLM calls for button clicks
4. **Session Management**: Sessions automatically cleaned up after inactivity

## Security

1. **Native Messaging**: Uses Firefox's secure native messaging protocol
2. **Session Isolation**: Each tab has isolated session with dedicated context
3. **Input Validation**: All protocol messages validated before processing
4. **Sandboxing**: WASM plugins run in sandboxed environment (future)

## Future Enhancements

1. **MCP Integration**: Connect to Model Context Protocol services
2. **WASM Plugins**: Support for WebAssembly plugins
3. **Browser Automation**: Full browser control API implementation
4. **Diff Updates**: Efficient UI updates using JSON diff
5. **Error Recovery**: Automatic reconnection and message replay

## License

Mozilla Public License 2.0

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for contribution guidelines.
