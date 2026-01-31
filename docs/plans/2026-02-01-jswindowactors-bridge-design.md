# JSWindowActors + WebExtension Bridge Design

> **Date:** 2026-02-01
> **Status:** Draft
> **Author:** Claude + User

## Overview

Design for integrating JSWindowActors with WebExtension to provide high-performance page interaction capabilities for NevoFlux Agent, while minimizing Zen browser patch maintenance burden.

### Goals

1. Enable AI Agent to read content from any tab (including discarded tabs)
2. Provide DevTools-like element picker for DOM selection
3. Real-time text selection synchronization
4. Page lock during Agent operations
5. Minimize patches to Zen browser source

### Non-Goals

- Full browser chrome integration (deferred to reduce maintenance)
- Custom protocol (`nevoflux://`) implementation (future phase)
- Canvas/Artifacts rendering (future phase)

---

## Architecture

### Hybrid Approach

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           NevoFlux Hybrid Architecture                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ WebExtension (UI Container)                                     │   │
│  │  ┌─────────────────────┐    ┌─────────────────────────────────┐ │   │
│  │  │ Dioxus Sidebar      │    │ background.js                   │ │   │
│  │  │ (WASM)              │    │ - Native Messaging → Rust Agent │ │   │
│  │  │                     │    │ - Existing Chat/MCP logic       │ │   │
│  │  │ New Components:     │    └─────────────────────────────────┘ │   │
│  │  │ - PickerButton      │                                        │   │
│  │  │ - TabReader         │    ┌─────────────────────────────────┐ │   │
│  │  │ - SelectionPanel    │    │ bridge.js                       │ │   │
│  │  │                     │◄──►│ window.NevofluxBridge           │ │   │
│  │  └─────────────────────┘    └───────────────┬─────────────────┘ │   │
│  │                                             │                    │   │
│  │  ┌──────────────────────────────────────────▼─────────────────┐ │   │
│  │  │ experiment_apis/nevoflux/                                  │ │   │
│  │  │ browser.nevoflux.* API                                     │ │   │
│  │  └──────────────────────────────────────────┬─────────────────┘ │   │
│  └─────────────────────────────────────────────┼───────────────────┘   │
│                                                │                        │
├────────────────────────────────────────────────┼────────────────────────┤
│  Browser Chrome (Minimal Patches)              │                        │
│  ┌─────────────────────────────────────────────▼─────────────────────┐ │
│  │ NevofluxActorManager                                              │ │
│  │  - Actor Registration                                             │ │
│  │  - NevofluxEventEmitter (Event Bus)                               │ │
│  └───────────────────────────────────────────────────────────────────┘ │
│                                                                         │
│  ┌───────────────────────────────┐    ┌────────────────────────────┐   │
│  │ NevofluxParent (Main Process) │◄──►│ NevofluxChild (Content)    │   │
│  │ - Picker Promise Management   │ IPC│ - DOM Selector Generation  │   │
│  │ - Event Forwarding            │    │ - Element Highlighting     │   │
│  │ - Tab Restore Logic           │    │ - Content Extraction       │   │
│  └───────────────────────────────┘    │ - Page Locking             │   │
│                                        └────────────────────────────┘   │
│  ┌───────────────────────────────────────────────────────────────────┐ │
│  │ zen-startup.patch (Only Patch Required)                           │ │
│  │ + lazy.NevofluxStartup.init();                                    │ │
│  └───────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Keep WebExtension as UI container | Minimizes browser.xhtml patches, easier Zen updates |
| Use JSWindowActors for page interaction | High-performance IPC, direct DOM access |
| Bridge via Experiment API | Standard extension mechanism, clean separation |
| Single startup patch | Minimal maintenance burden |

---

## API Design

### browser.nevoflux API

```typescript
interface NevofluxAPI {
  // Tab Content
  getTabContent(tabId: number, options?: {
    format: "markdown" | "html" | "text";
    selector?: string;
    autoRestore?: boolean;      // Auto-restore discarded tabs
    keepRestored?: boolean;     // Keep tab restored after extraction
    timeout?: number;
  }): Promise<TabContent>;

  getTabState(tabId: number): Promise<TabState>;

  // Element Picker
  pickElement(tabId: number, options?: {
    hint?: string;
    filter?: "any" | "button" | "input" | "link" | "image" | "clickable";
    timeout?: number;
    highlightColor?: string;
  }): Promise<PickerResult>;

  cancelPicker(tabId: number): Promise<void>;

  // Selection
  getSelection(tabId: number): Promise<SelectionData | null>;
  onSelectionChanged: Event<(tabId: number, selection: SelectionData | null) => void>;

  // Page Lock
  lockPage(tabId: number, options?: {
    showOverlay?: boolean;
    message?: string;
  }): Promise<void>;

  unlockPage(tabId: number): Promise<void>;
}
```

### Type Definitions

```typescript
interface TabContent {
  tabId: number;
  url: string;
  title: string;
  content: string;
  format: "markdown" | "html" | "text";
  extractedAt: number;
  wasDiscarded: boolean;
}

interface TabState {
  discarded: boolean;
  status: "complete" | "loading" | "unloaded";
  url: string;
  title: string;
}

interface PickerResult {
  selector: string;
  xpath: string;
  tagName: string;
  id: string | null;
  className: string | null;
  text: string | null;
  attributes: Record<string, string>;
  rect: { top: number; left: number; width: number; height: number };
}

interface SelectionData {
  text: string;
  html: string;
  rect: { top: number; left: number; width: number; height: number };
  anchorNode: string;
  url: string;
  title: string;
}
```

---

## Implementation Details

### 1. Tab Content Extraction with Auto-Restore

Discarded tabs have no content process, so we need to restore them before extraction.

```
┌─────────────────────────────────────────────────────────────────┐
│  getTabContent(tabId, { format: "markdown" })                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │ Tab discarded?  │
                    └────────┬────────┘
                             │
              ┌──────────────┴──────────────┐
              │ Yes                         │ No
              ▼                             ▼
    ┌─────────────────────┐      ┌─────────────────────┐
    │ 1. Silently restore │      │ Call Actor directly │
    │ 2. Wait for load    │      │ Extract content     │
    │ 3. Extract content  │      └──────────┬──────────┘
    │ 4. Re-discard tab   │                 │
    └──────────┬──────────┘                 │
               └────────────┬───────────────┘
                            ▼
                  ┌─────────────────┐
                  │ Return content  │
                  └─────────────────┘
```

**Key Implementation:**

```javascript
async _restoreTab(nativeTab, timeout) {
  // Use SessionStore.restoreTabContent() - does NOT switch visible tab
  const { SessionStore } = ChromeUtils.importESModule(
    "resource:///modules/sessionstore/SessionStore.sys.mjs"
  );

  return new Promise((resolve, reject) => {
    const onRestored = () => {
      nativeTab.removeEventListener("SSTabRestored", onRestored);
      setTimeout(resolve, 150); // Wait for Actor initialization
    };

    nativeTab.addEventListener("SSTabRestored", onRestored);
    SessionStore.restoreTabContent(nativeTab);
  });
}
```

### 2. Element Picker

DevTools-like element picker with capture-phase event interception.

**Features:**
- Visual highlight following mouse
- Element info label (tag, id, class)
- Multiple filter modes (any, button, input, link, image, clickable)
- Three selector strategies: ID → data-* → path
- XPath as fallback

**Selector Generation Priority:**

```javascript
_generateSelector(element) {
  // Priority 1: Unique ID
  if (element.id) {
    const selector = `#${CSS.escape(element.id)}`;
    if (document.querySelectorAll(selector).length === 1) {
      return selector;
    }
  }

  // Priority 2: data-testid or data-* attributes
  for (const attr of element.attributes) {
    if (attr.name === "data-testid" || attr.name.startsWith("data-")) {
      const selector = `[${attr.name}="${CSS.escape(attr.value)}"]`;
      if (document.querySelectorAll(selector).length === 1) {
        return selector;
      }
    }
  }

  // Priority 3: Build path with nth-of-type
  // ...
}
```

### 3. Selection Sync

Push-based selection synchronization with debouncing.

```javascript
// NevofluxChild.sys.mjs
actorCreated() {
  this.document.addEventListener("selectionchange", this);
  this.contentWindow.addEventListener("mouseup", this);
}

_debouncedPushSelection() {
  clearTimeout(this._selectionTimeout);
  this._selectionTimeout = setTimeout(() => this._pushSelection(), 150);
}
```

### 4. Page Lock

Dual-layer protection during Agent operations.

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 1: Visual (Overlay)                                  │
│  - Semi-transparent backdrop                                │
│  - Loading spinner + status message                         │
│  - Abort button                                             │
└─────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│  Layer 2: Logic (Event Lock)                                │
│  - Capture-phase event interception                         │
│  - Blocks all input events                                  │
│  - Prevents click-through                                   │
└─────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
src/nevoflux/
├── actors/
│   ├── NevofluxParent.sys.mjs        # Parent Actor (main process)
│   ├── NevofluxChild.sys.mjs         # Child Actor (content process)
│   └── NevofluxActorManager.sys.mjs  # Actor manager + event bus
│
├── modules/
│   └── NevofluxStartup.sys.mjs       # Startup initialization
│
├── experiment-apis/
│   └── nevoflux/
│       ├── api.sys.mjs               # Experiment API implementation
│       └── schema.json               # API schema
│
├── content/
│   └── lib/
│       ├── Readability.sys.mjs       # Mozilla Readability
│       └── Turndown.sys.mjs          # HTML→Markdown
│
├── patches/
│   └── zen-startup.patch             # Single startup patch (3 lines)
│
└── jar.mn                            # Chrome resource registration

src/nevoflux/extensions/nevoflux-agent/
├── manifest.json                     # Add experiment API
├── wasm/chat-sidebar/
│   └── bridge.js                     # JS Bridge for WASM
└── dioxus-ui/chat-sidebar/src/
    ├── bindings/
    │   └── nevoflux_api.rs           # Rust bindings
    ├── hooks/
    │   └── use_selection.rs          # Selection subscription hook
    └── components/
        ├── picker_button.rs          # Picker button component
        ├── tab_reader.rs             # Tab reader component
        └── selection_panel.rs        # Selection panel component
```

---

## Dioxus/WASM Integration

### JavaScript Bridge

```javascript
// bridge.js
window.NevofluxBridge = {
  async getTabContent(tabId, options) {
    try {
      const result = await browser.nevoflux.getTabContent(tabId, options);
      return { success: true, data: result };
    } catch (e) {
      return { success: false, error: e.message };
    }
  },
  // ... other methods
};
```

### Rust Bindings

```rust
// nevoflux_api.rs
pub async fn get_tab_content(tab_id: u32, options: Option<GetContentOptions>) -> Result<TabContent, String> {
    let options_js = match options {
        Some(opts) => serde_wasm_bindgen::to_value(&opts).unwrap_or(JsValue::NULL),
        None => JsValue::NULL,
    };
    call_api(js_get_tab_content(tab_id, options_js)).await
}
```

### Event Subscription

Uses `wasm_bindgen::Closure` to bridge JavaScript events to Rust:

```rust
let callback = Closure::wrap(Box::new(move |json: String| {
    if let Ok(event) = serde_json::from_str::<SelectionEvent>(&json) {
        selection.set(Some((event.tab_id, event.selection)));
    }
}) as Box<dyn Fn(String)>);

js_subscribe_selection(callback.as_ref().unchecked_ref());
callback.forget(); // Prevent deallocation
```

---

## Implementation Phases

### Phase 1: Infrastructure
- Create Actor file structure
- Implement NevofluxActorManager + event bus
- Create zen-startup.patch
- Update jar.mn
- Verify Actor registration

### Phase 2: Experiment API
- Create schema.json
- Implement api.sys.mjs skeleton
- Update extension manifest.json
- Implement getTabState()

### Phase 3: Tab Content
- Adapt Readability.js → .sys.mjs
- Adapt Turndown.js → .sys.mjs
- Implement content extraction
- Implement tab restore logic

### Phase 4: Element Picker
- Implement Child Actor picker logic
- Implement highlight UI
- Implement selector generation
- Implement Parent Actor promise management

### Phase 5: Selection Sync
- Implement selection monitoring
- Implement event push
- Implement API events

### Phase 6: Page Lock
- Implement event locking
- Implement visual overlay

### Phase 7: Dioxus Integration
- Create bridge.js
- Implement Rust bindings
- Implement hooks and components

### Phase 8: Documentation & Cleanup

---

## Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Readability.js ESModule conversion | Medium | High | Use Webpack or manual rewrite |
| Actor unavailable on some pages | Low | Medium | Exclude in matches, graceful degradation |
| Tab restore Actor init delay | Medium | Medium | Add retry logic + timeout |
| Picker conflicts with page events | Low | Low | Use capture phase + stopImmediatePropagation |

---

## Maintenance Impact

### Zen Update Conflict Risk

| Patch File | Risk | Reason |
|------------|------|--------|
| zen-startup.patch | Low | Only adds one function call |

### Estimated Maintenance per Zen Update

- **Best case** (no conflicts): Run `npm run import`, test → **30 min**
- **Typical case** (minor conflicts): Adjust patch → **1-2 hours**
- **Worst case** (architecture change): Rare, major rework needed

---

## Testing

```javascript
// Manual test script (Browser Console)

// Test 1: Actor registration
const browser = gBrowser.selectedBrowser;
const actor = browser.browsingContext.currentWindowGlobal.getActor("Nevoflux");
console.assert(actor, "Actor should be available");

// Test 2: Tab State
const state = await browser.nevoflux.getTabState(0);
console.log("Tab state:", state);

// Test 3: Tab Content
const content = await browser.nevoflux.getTabContent(0, { format: "markdown" });
console.log("Content length:", content.content.length);

// Test 4: Picker
const picked = await browser.nevoflux.pickElement(0, { hint: "Select an element" });
console.log("Picked:", picked.selector);

// Test 5: Selection
const sel = await browser.nevoflux.getSelection(0);
console.log("Selection:", sel?.text);

// Test 6: Page Lock
await browser.nevoflux.lockPage(0, { message: "Test lock" });
await browser.nevoflux.unlockPage(0);
```

---

## Future Considerations

1. **Custom Protocol (`nevoflux://`)** - For dashboard, settings, canvas pages
2. **Canvas/Artifacts Rendering** - Live HTML/React/Markdown rendering
3. **Plan Viewer** - Visual execution plan tracking
4. **ContentStore Data Bus** - Global state synchronization

These can be added incrementally without changing the core architecture.
