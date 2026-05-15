# Computer Use Guide

## Available Tools

| Tool | Purpose |
|------|---------|
| `computer_screenshot` | Capture the screen to observe current state |
| `computer_mouse_move` | Move cursor to (x, y) without clicking |
| `computer_click` | Click at (x, y) with button/click_type options |
| `computer_type_text` | Type text at the current cursor position |
| `computer_key` | Press key combinations (e.g., Ctrl+S, Enter) |
| `computer_scroll` | Scroll at (x, y) in a direction |
| `computer_drag` | Drag from (start_x, start_y) to (end_x, end_y) |
| `computer_cursor_position` | Get the current cursor position |
| `computer_mouse_down` | Press and hold a mouse button |
| `computer_mouse_up` | Release a held mouse button |
| `computer_hold_key` | Hold a key for a duration (e.g., long-press) |
| `computer_wait` | Wait for animations/loading (100-10000ms) |

## Operating Principles

### Observe Before Acting
Always take a `computer_screenshot` before interacting. Never guess coordinates — verify them visually.

### Act Then Verify
After each action, take another screenshot to confirm the expected result occurred.

### Wait for Readiness
Use `computer_wait` after actions that trigger animations, loading, or transitions. Check that the UI is stable before the next action.

### Confirm Focus Before Typing
Before using `computer_type_text`, click on the target input field first. Never type into an unfocused area.

### Prefer Keyboard Over Mouse
When possible, use keyboard shortcuts (`computer_key`) instead of clicking through menus. For example:
- `Ctrl+S` to save instead of File > Save
- `Tab` to move between form fields instead of clicking each one
- `Enter` to confirm dialogs instead of clicking OK

### Coordinate Precision
- Coordinates are in absolute screen pixels
- Click the center of the target element, not its edge
- For small targets, zoom in or use keyboard navigation instead
