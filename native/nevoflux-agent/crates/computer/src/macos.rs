//! macOS computer backend using Core Graphics.
//!
//! This module provides screenshot, mouse, and keyboard control functionality
//! for macOS using the Core Graphics and Core Foundation frameworks.

use crate::error::{ComputerError, Result};
use crate::traits::{ComputerController, KeyboardController, MouseController, ScreenshotProvider};
use crate::types::*;
use async_trait::async_trait;

#[cfg(target_os = "macos")]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
#[cfg(target_os = "macos")]
use core_graphics::display::{CGDisplay, CGDisplayBounds, CGMainDisplayID};
#[cfg(target_os = "macos")]
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, ScrollEventUnit,
};
#[cfg(target_os = "macos")]
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
#[cfg(target_os = "macos")]
use core_graphics::geometry::CGPoint;
#[cfg(target_os = "macos")]
use image::{ImageBuffer, Rgba};
#[cfg(target_os = "macos")]
use std::io::Cursor;

/// macOS computer controller using Core Graphics.
///
/// This struct is only available on macOS. On other platforms,
/// attempting to use it will result in `NotSupported` errors.
#[cfg(target_os = "macos")]
pub struct MacOsComputer {
    /// Event source for creating CGEvents.
    event_source: CGEventSource,
}

// SAFETY: CGEventSource is a CoreFoundation type with thread-safe reference counting.
// The CGEvent APIs we use are safe to call from any thread.
#[cfg(target_os = "macos")]
unsafe impl Send for MacOsComputer {}
#[cfg(target_os = "macos")]
unsafe impl Sync for MacOsComputer {}

/// Stub MacOsComputer for non-macOS platforms.
/// All operations return NotSupported errors.
#[cfg(not(target_os = "macos"))]
pub struct MacOsComputer {
    _private: (),
}

#[cfg(target_os = "macos")]
impl MacOsComputer {
    /// Create a new macOS computer controller.
    pub fn new() -> Result<Self> {
        let event_source =
            CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
                ComputerError::ConnectionFailed(
                    "Failed to create CGEventSource for macOS input".into(),
                )
            })?;

        Ok(Self { event_source })
    }

    /// Get the main display ID.
    fn main_display_id() -> u32 {
        unsafe { CGMainDisplayID() }
    }

    /// Convert raw image data to PNG and encode as base64.
    fn encode_to_png_base64(&self, data: &[u8], width: u32, height: u32) -> Result<String> {
        // Core Graphics returns BGRA format (or RGBA depending on configuration)
        // We need to convert to RGBA for PNG encoding

        // Validate data size - must be a multiple of 4 (BGRA = 4 bytes per pixel)
        if data.len() % 4 != 0 {
            return Err(ComputerError::ScreenshotFailed(format!(
                "Invalid image data length: {} (not a multiple of 4)",
                data.len()
            )));
        }

        // Create image buffer (assuming RGBA from Core Graphics capture)
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(width, height, data.to_vec()).ok_or_else(|| {
                ComputerError::ImageEncoding("Failed to create image buffer".into())
            })?;

        // Encode to PNG
        let mut png_data = Vec::new();
        let mut cursor = Cursor::new(&mut png_data);
        img.write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| ComputerError::ImageEncoding(format!("Failed to encode PNG: {}", e)))?;

        // Encode to base64
        Ok(BASE64_STANDARD.encode(&png_data))
    }

    /// Convert a Key to a macOS virtual key code.
    fn key_to_keycode(key: &Key) -> u16 {
        // macOS virtual key codes from Carbon HIToolbox/Events.h
        match key {
            // Modifiers
            Key::Shift => 0x38,   // kVK_Shift
            Key::Control => 0x3B, // kVK_Control
            Key::Alt => 0x3A,     // kVK_Option
            Key::Meta => 0x37,    // kVK_Command

            // Function keys
            Key::F1 => 0x7A,
            Key::F2 => 0x78,
            Key::F3 => 0x63,
            Key::F4 => 0x76,
            Key::F5 => 0x60,
            Key::F6 => 0x61,
            Key::F7 => 0x62,
            Key::F8 => 0x64,
            Key::F9 => 0x65,
            Key::F10 => 0x6D,
            Key::F11 => 0x67,
            Key::F12 => 0x6F,

            // Navigation
            Key::Escape => 0x35,
            Key::Tab => 0x30,
            Key::CapsLock => 0x39,
            Key::Space => 0x31,
            Key::Enter => 0x24,     // kVK_Return
            Key::Backspace => 0x33, // kVK_Delete
            Key::Delete => 0x75,    // kVK_ForwardDelete
            Key::Insert => 0x72,    // kVK_Help (no dedicated Insert key on Mac)
            Key::Home => 0x73,
            Key::End => 0x77,
            Key::PageUp => 0x74,
            Key::PageDown => 0x79,
            Key::ArrowUp => 0x7E,
            Key::ArrowDown => 0x7D,
            Key::ArrowLeft => 0x7B,
            Key::ArrowRight => 0x7C,

            // Other
            Key::PrintScreen => 0x69, // No direct equivalent, using F13
            Key::ScrollLock => 0x6B,  // No direct equivalent, using F14
            Key::Pause => 0x71,       // No direct equivalent, using F15
            Key::NumLock => 0x47,     // kVK_ANSI_KeypadClear
        }
    }

    /// Convert a character to a macOS virtual key code.
    /// Returns (keycode, needs_shift).
    fn char_to_keycode(c: char) -> Option<(u16, bool)> {
        // Map common ASCII characters to their virtual key codes
        // This is a simplified mapping - a full implementation would use
        // the Carbon Text Input Sources API
        match c {
            'a' | 'A' => Some((0x00, c.is_uppercase())),
            'b' | 'B' => Some((0x0B, c.is_uppercase())),
            'c' | 'C' => Some((0x08, c.is_uppercase())),
            'd' | 'D' => Some((0x02, c.is_uppercase())),
            'e' | 'E' => Some((0x0E, c.is_uppercase())),
            'f' | 'F' => Some((0x03, c.is_uppercase())),
            'g' | 'G' => Some((0x05, c.is_uppercase())),
            'h' | 'H' => Some((0x04, c.is_uppercase())),
            'i' | 'I' => Some((0x22, c.is_uppercase())),
            'j' | 'J' => Some((0x26, c.is_uppercase())),
            'k' | 'K' => Some((0x28, c.is_uppercase())),
            'l' | 'L' => Some((0x25, c.is_uppercase())),
            'm' | 'M' => Some((0x2E, c.is_uppercase())),
            'n' | 'N' => Some((0x2D, c.is_uppercase())),
            'o' | 'O' => Some((0x1F, c.is_uppercase())),
            'p' | 'P' => Some((0x23, c.is_uppercase())),
            'q' | 'Q' => Some((0x0C, c.is_uppercase())),
            'r' | 'R' => Some((0x0F, c.is_uppercase())),
            's' | 'S' => Some((0x01, c.is_uppercase())),
            't' | 'T' => Some((0x11, c.is_uppercase())),
            'u' | 'U' => Some((0x20, c.is_uppercase())),
            'v' | 'V' => Some((0x09, c.is_uppercase())),
            'w' | 'W' => Some((0x0D, c.is_uppercase())),
            'x' | 'X' => Some((0x07, c.is_uppercase())),
            'y' | 'Y' => Some((0x10, c.is_uppercase())),
            'z' | 'Z' => Some((0x06, c.is_uppercase())),
            '0' | ')' => Some((0x1D, c == ')')),
            '1' | '!' => Some((0x12, c == '!')),
            '2' | '@' => Some((0x13, c == '@')),
            '3' | '#' => Some((0x14, c == '#')),
            '4' | '$' => Some((0x15, c == '$')),
            '5' | '%' => Some((0x17, c == '%')),
            '6' | '^' => Some((0x16, c == '^')),
            '7' | '&' => Some((0x1A, c == '&')),
            '8' | '*' => Some((0x1C, c == '*')),
            '9' | '(' => Some((0x19, c == '(')),
            ' ' => Some((0x31, false)),
            '-' | '_' => Some((0x1B, c == '_')),
            '=' | '+' => Some((0x18, c == '+')),
            '[' | '{' => Some((0x21, c == '{')),
            ']' | '}' => Some((0x1E, c == '}')),
            '\\' | '|' => Some((0x2A, c == '|')),
            ';' | ':' => Some((0x29, c == ':')),
            '\'' | '"' => Some((0x27, c == '"')),
            ',' | '<' => Some((0x2B, c == '<')),
            '.' | '>' => Some((0x2F, c == '>')),
            '/' | '?' => Some((0x2C, c == '?')),
            '`' | '~' => Some((0x32, c == '~')),
            '\n' => Some((0x24, false)), // Return
            '\t' => Some((0x30, false)), // Tab
            _ => None,
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl MacOsComputer {
    /// Create a new macOS computer controller.
    /// On non-macOS platforms, this always returns a NotSupported error.
    pub fn new() -> Result<Self> {
        Err(ComputerError::NotSupported(
            "MacOsComputer is only available on macOS".into(),
        ))
    }
}

#[cfg(target_os = "macos")]
impl Default for MacOsComputer {
    fn default() -> Self {
        Self::new().expect("Failed to create macOS controller")
    }
}

// ============================================================================
// macOS implementations
// ============================================================================

#[cfg(target_os = "macos")]
#[async_trait]
impl ScreenshotProvider for MacOsComputer {
    async fn capture_screen(&self) -> Result<Screenshot> {
        // Get main display bounds
        let display_id = Self::main_display_id();
        let bounds = unsafe { CGDisplayBounds(display_id as u32) };

        let width = bounds.size.width as u32;
        let height = bounds.size.height as u32;

        // Capture the display
        let display = CGDisplay::new(display_id);
        let image = display
            .image()
            .ok_or_else(|| ComputerError::ScreenshotFailed("Failed to capture display".into()))?;

        // Get image data
        let data = image.data();

        let base64_data = self.encode_to_png_base64(&data, width, height)?;

        Ok(Screenshot::new(
            width,
            height,
            ImageFormat::Png,
            base64_data,
        ))
    }

    async fn capture_display(&self, display_id: u32) -> Result<Screenshot> {
        let bounds = unsafe { CGDisplayBounds(display_id) };

        let width = bounds.size.width as u32;
        let height = bounds.size.height as u32;

        let display = CGDisplay::new(display_id);
        let image = display
            .image()
            .ok_or_else(|| ComputerError::ScreenshotFailed("Failed to capture display".into()))?;

        let data = image.data();

        let base64_data = self.encode_to_png_base64(&data, width, height)?;

        Ok(Screenshot::new(
            width,
            height,
            ImageFormat::Png,
            base64_data,
        ))
    }

    async fn capture_region(&self, region: Region) -> Result<Screenshot> {
        // Capture the full screen and crop to region
        let display_id = Self::main_display_id();

        // Use CGDisplayCreateImageForRect for region capture
        let cg_rect = core_graphics::geometry::CGRect::new(
            &CGPoint::new(region.x as f64, region.y as f64),
            &core_graphics::geometry::CGSize::new(region.width as f64, region.height as f64),
        );

        let image = CGDisplay::screenshot(cg_rect, 0, 0, display_id)
            .ok_or_else(|| ComputerError::ScreenshotFailed("Failed to capture region".into()))?;

        let data = image.data();

        let base64_data = self.encode_to_png_base64(&data, region.width, region.height)?;

        Ok(Screenshot::new(
            region.width,
            region.height,
            ImageFormat::Png,
            base64_data,
        ))
    }

    async fn get_displays(&self) -> Result<Vec<DisplayInfo>> {
        let display_ids = CGDisplay::active_displays().map_err(|e| {
            ComputerError::ScreenshotFailed(format!("Failed to get displays: {}", e))
        })?;

        let main_id = Self::main_display_id();
        let mut displays = Vec::new();

        for (index, &display_id) in display_ids.iter().enumerate() {
            let bounds = unsafe { CGDisplayBounds(display_id) };

            displays.push(DisplayInfo {
                id: display_id,
                name: None,
                is_primary: display_id == main_id,
                bounds: Region::new(
                    bounds.origin.x as i32,
                    bounds.origin.y as i32,
                    bounds.size.width as u32,
                    bounds.size.height as u32,
                ),
                scale_factor: 1.0, // Could use NSScreen to get actual scale
            });
        }

        if displays.is_empty() {
            // Fallback to main display
            let bounds = unsafe { CGDisplayBounds(main_id) };
            displays.push(DisplayInfo::primary(
                bounds.size.width as u32,
                bounds.size.height as u32,
            ));
        }

        Ok(displays)
    }
}

#[cfg(target_os = "macos")]
#[async_trait]
impl MouseController for MacOsComputer {
    async fn get_position(&self) -> Result<Point> {
        // Get current mouse location using CGEvent
        let event = CGEvent::new(self.event_source.clone()).map_err(|_| {
            ComputerError::MouseFailed("Failed to create CGEvent for position query".into())
        })?;

        let location = event.location();
        Ok(Point::new(location.x as i32, location.y as i32))
    }

    async fn move_to(&self, point: Point) -> Result<()> {
        let position = CGPoint::new(point.x as f64, point.y as f64);

        let event = CGEvent::new_mouse_event(
            self.event_source.clone(),
            CGEventType::MouseMoved,
            position,
            CGMouseButton::Left,
        )
        .map_err(|_| ComputerError::MouseFailed("Failed to create mouse move event".into()))?;

        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    async fn move_by(&self, dx: i32, dy: i32) -> Result<()> {
        let current = self.get_position().await?;
        let new_point = Point::new(current.x + dx, current.y + dy);
        self.move_to(new_point).await
    }

    async fn click(&self, button: MouseButton, click_type: ClickType) -> Result<()> {
        let click_count = match click_type {
            ClickType::Single => 1,
            ClickType::Double => 2,
            ClickType::Triple => 3,
        };

        for _ in 0..click_count {
            self.press(button).await?;
            self.release(button).await?;
        }

        Ok(())
    }

    async fn press(&self, button: MouseButton) -> Result<()> {
        let current = self.get_position().await?;
        let position = CGPoint::new(current.x as f64, current.y as f64);

        let (event_type, cg_button) = match button {
            MouseButton::Left => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            MouseButton::Right => (CGEventType::RightMouseDown, CGMouseButton::Right),
            MouseButton::Middle => (CGEventType::OtherMouseDown, CGMouseButton::Center),
        };

        let event =
            CGEvent::new_mouse_event(self.event_source.clone(), event_type, position, cg_button)
                .map_err(|_| {
                    ComputerError::MouseFailed("Failed to create mouse press event".into())
                })?;

        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    async fn release(&self, button: MouseButton) -> Result<()> {
        let current = self.get_position().await?;
        let position = CGPoint::new(current.x as f64, current.y as f64);

        let (event_type, cg_button) = match button {
            MouseButton::Left => (CGEventType::LeftMouseUp, CGMouseButton::Left),
            MouseButton::Right => (CGEventType::RightMouseUp, CGMouseButton::Right),
            MouseButton::Middle => (CGEventType::OtherMouseUp, CGMouseButton::Center),
        };

        let event =
            CGEvent::new_mouse_event(self.event_source.clone(), event_type, position, cg_button)
                .map_err(|_| {
                    ComputerError::MouseFailed("Failed to create mouse release event".into())
                })?;

        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    async fn scroll(&self, direction: ScrollDirection, amount: u32) -> Result<()> {
        let (delta_x, delta_y) = match direction {
            ScrollDirection::Up => (0, amount as i32),
            ScrollDirection::Down => (0, -(amount as i32)),
            ScrollDirection::Left => (-(amount as i32), 0),
            ScrollDirection::Right => (amount as i32, 0),
        };

        let event = CGEvent::new_scroll_event(
            self.event_source.clone(),
            ScrollEventUnit::LINE,
            2,
            delta_y,
            delta_x,
            0,
        )
        .map_err(|_| ComputerError::MouseFailed("Failed to create scroll event".into()))?;

        event.post(CGEventTapLocation::HID);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
#[async_trait]
impl KeyboardController for MacOsComputer {
    async fn type_text(&self, text: &str) -> Result<()> {
        for c in text.chars() {
            let combo = KeyCombination::char(c);
            self.press_key(combo).await?;
        }
        Ok(())
    }

    async fn press_key(&self, combination: KeyCombination) -> Result<()> {
        // Press modifiers first
        for modifier in &combination.modifiers {
            let keycode = Self::key_to_keycode(modifier);
            let event = CGEvent::new_keyboard_event(self.event_source.clone(), keycode, true)
                .map_err(|_| {
                    ComputerError::KeyboardFailed("Failed to create key down event".into())
                })?;
            event.post(CGEventTapLocation::HID);
        }

        // Press and release the main key
        let keycode = match &combination.key {
            KeyOrChar::Key(k) => Self::key_to_keycode(k),
            KeyOrChar::Char(c) => {
                if let Some((kc, needs_shift)) = Self::char_to_keycode(*c) {
                    if needs_shift {
                        // Press shift if needed for this character
                        let shift_event =
                            CGEvent::new_keyboard_event(self.event_source.clone(), 0x38, true)
                                .map_err(|_| {
                                    ComputerError::KeyboardFailed(
                                        "Failed to create shift key event".into(),
                                    )
                                })?;
                        shift_event.post(CGEventTapLocation::HID);
                    }
                    kc
                } else {
                    return Err(ComputerError::InvalidKey(format!(
                        "Unsupported character: {}",
                        c
                    )));
                }
            }
        };

        // Key down
        let down_event = CGEvent::new_keyboard_event(self.event_source.clone(), keycode, true)
            .map_err(|_| ComputerError::KeyboardFailed("Failed to create key down event".into()))?;
        down_event.post(CGEventTapLocation::HID);

        // Key up
        let up_event = CGEvent::new_keyboard_event(self.event_source.clone(), keycode, false)
            .map_err(|_| ComputerError::KeyboardFailed("Failed to create key up event".into()))?;
        up_event.post(CGEventTapLocation::HID);

        // Release shift if we pressed it for a character
        if let KeyOrChar::Char(c) = &combination.key {
            if let Some((_, needs_shift)) = Self::char_to_keycode(*c) {
                if needs_shift {
                    let shift_up =
                        CGEvent::new_keyboard_event(self.event_source.clone(), 0x38, false)
                            .map_err(|_| {
                                ComputerError::KeyboardFailed(
                                    "Failed to create shift key up event".into(),
                                )
                            })?;
                    shift_up.post(CGEventTapLocation::HID);
                }
            }
        }

        // Release modifiers in reverse order
        for modifier in combination.modifiers.iter().rev() {
            let keycode = Self::key_to_keycode(modifier);
            let event = CGEvent::new_keyboard_event(self.event_source.clone(), keycode, false)
                .map_err(|_| {
                    ComputerError::KeyboardFailed("Failed to create key up event".into())
                })?;
            event.post(CGEventTapLocation::HID);
        }

        Ok(())
    }

    async fn key_down(&self, combination: KeyCombination) -> Result<()> {
        // Press modifiers first
        for modifier in &combination.modifiers {
            let keycode = Self::key_to_keycode(modifier);
            let event = CGEvent::new_keyboard_event(self.event_source.clone(), keycode, true)
                .map_err(|_| {
                    ComputerError::KeyboardFailed("Failed to create key down event".into())
                })?;
            event.post(CGEventTapLocation::HID);
        }

        // Press the main key (hold down)
        let keycode = match &combination.key {
            KeyOrChar::Key(k) => Self::key_to_keycode(k),
            KeyOrChar::Char(c) => Self::char_to_keycode(*c).map(|(kc, _)| kc).ok_or_else(|| {
                ComputerError::InvalidKey(format!("Unsupported character: {}", c))
            })?,
        };

        let event = CGEvent::new_keyboard_event(self.event_source.clone(), keycode, true)
            .map_err(|_| ComputerError::KeyboardFailed("Failed to create key down event".into()))?;
        event.post(CGEventTapLocation::HID);

        Ok(())
    }

    async fn key_up(&self, combination: KeyCombination) -> Result<()> {
        // Release the main key first
        let keycode = match &combination.key {
            KeyOrChar::Key(k) => Self::key_to_keycode(k),
            KeyOrChar::Char(c) => Self::char_to_keycode(*c).map(|(kc, _)| kc).ok_or_else(|| {
                ComputerError::InvalidKey(format!("Unsupported character: {}", c))
            })?,
        };

        let event = CGEvent::new_keyboard_event(self.event_source.clone(), keycode, false)
            .map_err(|_| ComputerError::KeyboardFailed("Failed to create key up event".into()))?;
        event.post(CGEventTapLocation::HID);

        // Release modifiers in reverse order
        for modifier in combination.modifiers.iter().rev() {
            let keycode = Self::key_to_keycode(modifier);
            let event = CGEvent::new_keyboard_event(self.event_source.clone(), keycode, false)
                .map_err(|_| {
                    ComputerError::KeyboardFailed("Failed to create key up event".into())
                })?;
            event.post(CGEventTapLocation::HID);
        }

        Ok(())
    }
}

#[cfg(target_os = "macos")]
#[async_trait]
impl ComputerController for MacOsComputer {
    fn name(&self) -> &str {
        "macos-coregraphics"
    }

    fn is_available(&self) -> bool {
        // Core Graphics is always available on macOS
        true
    }
}

// ============================================================================
// Stub implementations for non-macOS platforms
// ============================================================================

#[cfg(not(target_os = "macos"))]
#[async_trait]
impl ScreenshotProvider for MacOsComputer {
    async fn capture_screen(&self) -> Result<Screenshot> {
        Err(ComputerError::NotSupported(
            "Screenshot capture is only available on macOS".into(),
        ))
    }

    async fn capture_display(&self, _display_id: u32) -> Result<Screenshot> {
        Err(ComputerError::NotSupported(
            "Display capture is only available on macOS".into(),
        ))
    }

    async fn capture_region(&self, _region: Region) -> Result<Screenshot> {
        Err(ComputerError::NotSupported(
            "Region capture is only available on macOS".into(),
        ))
    }

    async fn get_displays(&self) -> Result<Vec<DisplayInfo>> {
        Err(ComputerError::NotSupported(
            "Display enumeration is only available on macOS".into(),
        ))
    }
}

#[cfg(not(target_os = "macos"))]
#[async_trait]
impl MouseController for MacOsComputer {
    async fn get_position(&self) -> Result<Point> {
        Err(ComputerError::NotSupported(
            "Mouse position is only available on macOS".into(),
        ))
    }

    async fn move_to(&self, _point: Point) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse movement is only available on macOS".into(),
        ))
    }

    async fn move_by(&self, _dx: i32, _dy: i32) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse movement is only available on macOS".into(),
        ))
    }

    async fn click(&self, _button: MouseButton, _click_type: ClickType) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse click is only available on macOS".into(),
        ))
    }

    async fn press(&self, _button: MouseButton) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse press is only available on macOS".into(),
        ))
    }

    async fn release(&self, _button: MouseButton) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse release is only available on macOS".into(),
        ))
    }

    async fn scroll(&self, _direction: ScrollDirection, _amount: u32) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse scroll is only available on macOS".into(),
        ))
    }
}

#[cfg(not(target_os = "macos"))]
#[async_trait]
impl KeyboardController for MacOsComputer {
    async fn type_text(&self, _text: &str) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Keyboard input is only available on macOS".into(),
        ))
    }

    async fn press_key(&self, _combination: KeyCombination) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Keyboard input is only available on macOS".into(),
        ))
    }

    async fn key_down(&self, _combination: KeyCombination) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Keyboard input is only available on macOS".into(),
        ))
    }

    async fn key_up(&self, _combination: KeyCombination) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Keyboard input is only available on macOS".into(),
        ))
    }
}

#[cfg(not(target_os = "macos"))]
#[async_trait]
impl ComputerController for MacOsComputer {
    fn name(&self) -> &str {
        "macos-coregraphics"
    }

    fn is_available(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_computer_creation() {
        // On non-macOS, creation should fail with NotSupported
        #[cfg(not(target_os = "macos"))]
        {
            let result = MacOsComputer::new();
            assert!(result.is_err());
            if let Err(ComputerError::NotSupported(msg)) = result {
                assert!(msg.contains("macOS"));
            } else {
                panic!("Expected NotSupported error");
            }
        }

        // On macOS, creation should succeed
        #[cfg(target_os = "macos")]
        {
            let result = MacOsComputer::new();
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_macos_computer_name() {
        #[cfg(target_os = "macos")]
        {
            if let Ok(computer) = MacOsComputer::new() {
                assert_eq!(computer.name(), "macos-coregraphics");
            }
        }
    }

    #[test]
    fn test_macos_is_available() {
        #[cfg(target_os = "macos")]
        {
            if let Ok(computer) = MacOsComputer::new() {
                assert!(computer.is_available());
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            // Can't test is_available on non-macOS since new() fails
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_key_to_keycode() {
        // Test modifier keys
        assert_eq!(MacOsComputer::key_to_keycode(&Key::Shift), 0x38);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::Control), 0x3B);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::Alt), 0x3A);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::Meta), 0x37);

        // Test function keys
        assert_eq!(MacOsComputer::key_to_keycode(&Key::F1), 0x7A);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::F12), 0x6F);

        // Test navigation keys
        assert_eq!(MacOsComputer::key_to_keycode(&Key::Escape), 0x35);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::Enter), 0x24);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::Space), 0x31);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::Tab), 0x30);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::Backspace), 0x33);

        // Test arrow keys
        assert_eq!(MacOsComputer::key_to_keycode(&Key::ArrowUp), 0x7E);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::ArrowDown), 0x7D);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::ArrowLeft), 0x7B);
        assert_eq!(MacOsComputer::key_to_keycode(&Key::ArrowRight), 0x7C);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_char_to_keycode() {
        // Test lowercase letters
        assert_eq!(MacOsComputer::char_to_keycode('a'), Some((0x00, false)));
        assert_eq!(MacOsComputer::char_to_keycode('z'), Some((0x06, false)));

        // Test uppercase letters (needs shift)
        assert_eq!(MacOsComputer::char_to_keycode('A'), Some((0x00, true)));
        assert_eq!(MacOsComputer::char_to_keycode('Z'), Some((0x06, true)));

        // Test numbers
        assert_eq!(MacOsComputer::char_to_keycode('0'), Some((0x1D, false)));
        assert_eq!(MacOsComputer::char_to_keycode('9'), Some((0x19, false)));

        // Test shifted numbers (symbols)
        assert_eq!(MacOsComputer::char_to_keycode('!'), Some((0x12, true)));
        assert_eq!(MacOsComputer::char_to_keycode('@'), Some((0x13, true)));

        // Test special characters
        assert_eq!(MacOsComputer::char_to_keycode(' '), Some((0x31, false)));
        assert_eq!(MacOsComputer::char_to_keycode('\n'), Some((0x24, false)));
        assert_eq!(MacOsComputer::char_to_keycode('\t'), Some((0x30, false)));

        // Test unsupported character
        assert_eq!(MacOsComputer::char_to_keycode('\u{1F600}'), None);
    }

    #[tokio::test]
    async fn test_non_macos_operations_return_not_supported() {
        #[cfg(not(target_os = "macos"))]
        {
            // We can't create a MacOsComputer on non-macOS, but we can test
            // that the error handling is correct
            let result = MacOsComputer::new();
            assert!(matches!(result, Err(ComputerError::NotSupported(_))));
        }
    }
}
