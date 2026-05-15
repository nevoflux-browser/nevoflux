# Platform Support

## Linux

### X11 (Fully Supported)
- Screenshot: Full screen, region, display
- Mouse: Move, click, drag, scroll
- Keyboard: Type text, key combinations

### Wayland (Limited Support)
Wayland's security model restricts global input access:
- Screenshot: Requires portal or compositor support
- Mouse: No global control (security restriction)
- Keyboard: No global control (security restriction)

**Detection**: NevoFlux detects Wayland via `WAYLAND_DISPLAY` and `XDG_SESSION_TYPE` environment variables.

**Workaround**: Run with XWayland for full functionality. Most Wayland compositors include XWayland support.

## macOS

Fully supported via Core Graphics:
- Requires Accessibility permissions
- Grant in System Preferences > Security & Privacy > Privacy > Accessibility

## Windows

Fully supported via Win32 API:
- Screenshot: GDI capture
- Input: SendInput API

## Feature Matrix

| Feature | Linux X11 | Linux Wayland | macOS | Windows |
|---------|-----------|---------------|-------|---------|
| Screenshot | Yes | Portal | Yes | Yes |
| Mouse Move | Yes | No | Yes | Yes |
| Mouse Click | Yes | No | Yes | Yes |
| Keyboard | Yes | No | Yes | Yes |
| Multi-monitor | Yes | Limited | Yes | Yes |
