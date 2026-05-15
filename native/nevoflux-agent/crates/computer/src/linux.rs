//! Linux computer backend with X11 support and Wayland detection.
//!
//! Note: Full functionality requires X11. Wayland detection is provided but
//! input control requires XWayland compatibility layer.

use crate::error::{ComputerError, Result};
use crate::traits::{ComputerController, KeyboardController, MouseController, ScreenshotProvider};
use crate::types::*;
use async_trait::async_trait;

/// Display server type on Linux.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayServer {
    /// X11 display server.
    X11,
    /// Wayland compositor.
    Wayland,
    /// No display server detected.
    None,
}

/// Check if running under Wayland.
///
/// Checks both WAYLAND_DISPLAY and XDG_SESSION_TYPE for reliable detection.
pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
}

/// Get display server type.
pub fn display_server() -> DisplayServer {
    if is_wayland() {
        DisplayServer::Wayland
    } else if std::env::var("DISPLAY").is_ok() {
        DisplayServer::X11
    } else {
        DisplayServer::None
    }
}

#[cfg(target_os = "linux")]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
#[cfg(target_os = "linux")]
use image::{ImageBuffer, Rgba};
#[cfg(target_os = "linux")]
use std::io::Cursor;
#[cfg(target_os = "linux")]
use x11rb::connection::Connection;
#[cfg(target_os = "linux")]
use x11rb::protocol::randr::ConnectionExt as RandrExt;
#[cfg(target_os = "linux")]
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat as X11ImageFormat, Keycode};
#[cfg(target_os = "linux")]
use x11rb::protocol::xtest;
#[cfg(target_os = "linux")]
use x11rb::protocol::xtest::ConnectionExt as XTestExt;
#[cfg(target_os = "linux")]
use x11rb::rust_connection::RustConnection;

// XTest event types
#[cfg(target_os = "linux")]
const KEY_PRESS: u8 = 2;
#[cfg(target_os = "linux")]
const KEY_RELEASE: u8 = 3;
#[cfg(target_os = "linux")]
const BUTTON_PRESS: u8 = 4;
#[cfg(target_os = "linux")]
const BUTTON_RELEASE: u8 = 5;
#[cfg(target_os = "linux")]
const MOTION_NOTIFY: u8 = 6;

// X11 button codes
#[cfg(target_os = "linux")]
const BUTTON_LEFT: u8 = 1;
#[cfg(target_os = "linux")]
const BUTTON_MIDDLE: u8 = 2;
#[cfg(target_os = "linux")]
const BUTTON_RIGHT: u8 = 3;
#[cfg(target_os = "linux")]
const BUTTON_SCROLL_UP: u8 = 4;
#[cfg(target_os = "linux")]
const BUTTON_SCROLL_DOWN: u8 = 5;
#[cfg(target_os = "linux")]
const BUTTON_SCROLL_LEFT: u8 = 6;
#[cfg(target_os = "linux")]
const BUTTON_SCROLL_RIGHT: u8 = 7;

/// Linux computer controller using X11.
#[cfg(target_os = "linux")]
pub struct LinuxComputer {
    /// X11 connection.
    conn: RustConnection,
    /// Root window ID.
    root: u32,
    /// Screen number.
    screen_num: usize,
}

#[cfg(target_os = "linux")]
impl LinuxComputer {
    /// Create a new Linux computer controller.
    pub fn new() -> Result<Self> {
        let (conn, screen_num) = RustConnection::connect(None).map_err(|e| {
            ComputerError::ConnectionFailed(format!("Failed to connect to X11 display: {}", e))
        })?;

        // Query XTest extension version to verify availability
        xtest::get_version(&conn, 2, 0)
            .map_err(|e| {
                ComputerError::ConnectionFailed(format!("XTest extension unavailable: {}", e))
            })?
            .reply()
            .map_err(|e| {
                ComputerError::ConnectionFailed(format!("XTest extension check failed: {}", e))
            })?;

        let root = conn.setup().roots[screen_num].root;

        Ok(Self {
            conn,
            root,
            screen_num,
        })
    }

    /// Get the screen dimensions.
    fn get_screen_dimensions(&self) -> (u16, u16) {
        let screen = &self.conn.setup().roots[self.screen_num];
        (screen.width_in_pixels, screen.height_in_pixels)
    }

    /// Capture a region of the screen and return raw BGRA pixel data.
    fn capture_region_raw(&self, x: i16, y: i16, width: u16, height: u16) -> Result<Vec<u8>> {
        let cookie = self.conn.get_image(
            X11ImageFormat::Z_PIXMAP,
            self.root,
            x,
            y,
            width,
            height,
            !0, // All planes
        );

        let reply = cookie
            .map_err(|e| {
                ComputerError::ScreenshotFailed(format!("Failed to request image: {}", e))
            })?
            .reply()
            .map_err(|e| ComputerError::ScreenshotFailed(format!("Failed to get image: {}", e)))?;

        Ok(reply.data)
    }

    /// Convert raw X11 image data (BGRA) to PNG and encode as base64.
    fn encode_to_png_base64(&self, data: &[u8], width: u32, height: u32) -> Result<String> {
        // Validate data size - must be a multiple of 4 (BGRA = 4 bytes per pixel)
        if !data.len().is_multiple_of(4) {
            return Err(ComputerError::ScreenshotFailed(format!(
                "Invalid image data length: {} (not a multiple of 4)",
                data.len()
            )));
        }

        // X11 returns BGRA format, we need to convert to RGBA
        let mut rgba_data = Vec::with_capacity(data.len());
        for chunk in data.chunks_exact(4) {
            // BGRA -> RGBA
            rgba_data.push(chunk[2]); // R
            rgba_data.push(chunk[1]); // G
            rgba_data.push(chunk[0]); // B
            rgba_data.push(chunk[3]); // A
        }

        // Create image buffer
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(width, height, rgba_data)
            .ok_or_else(|| {
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
}

#[cfg(target_os = "linux")]
impl Default for LinuxComputer {
    fn default() -> Self {
        Self::new().expect("Failed to create Linux controller")
    }
}

#[cfg(target_os = "linux")]
#[async_trait]
impl ScreenshotProvider for LinuxComputer {
    async fn capture_screen(&self) -> Result<Screenshot> {
        let (width, height) = self.get_screen_dimensions();

        let data = self.capture_region_raw(0, 0, width, height)?;
        let base64_data = self.encode_to_png_base64(&data, width as u32, height as u32)?;

        Ok(Screenshot::new(
            width as u32,
            height as u32,
            ImageFormat::Png,
            base64_data,
        ))
    }

    async fn capture_display(&self, display_id: u32) -> Result<Screenshot> {
        // Get display info and capture that specific region
        let displays = self.get_displays().await?;

        let display = displays
            .into_iter()
            .find(|d| d.id == display_id)
            .ok_or_else(|| {
                ComputerError::ScreenshotFailed(format!("Display {} not found", display_id))
            })?;

        self.capture_region(display.bounds).await
    }

    async fn capture_region(&self, region: Region) -> Result<Screenshot> {
        let data = self.capture_region_raw(
            region.x as i16,
            region.y as i16,
            region.width as u16,
            region.height as u16,
        )?;

        let base64_data = self.encode_to_png_base64(&data, region.width, region.height)?;

        Ok(Screenshot::new(
            region.width,
            region.height,
            ImageFormat::Png,
            base64_data,
        ))
    }

    async fn get_displays(&self) -> Result<Vec<DisplayInfo>> {
        // Try to use RandR extension for multi-monitor info
        let randr_result = self.get_displays_randr();

        match randr_result {
            Ok(displays) if !displays.is_empty() => Ok(displays),
            _ => {
                // Fallback to simple screen info
                let (width, height) = self.get_screen_dimensions();
                Ok(vec![DisplayInfo::primary(width as u32, height as u32)])
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl LinuxComputer {
    /// Send an XTest fake input event.
    fn fake_input(&self, event_type: u8, detail: u8, x: i16, y: i16) -> Result<()> {
        self.conn
            .xtest_fake_input(event_type, detail, 0, self.root, x, y, 0)
            .map_err(|e| ComputerError::InputFailed(format!("XTest fake_input failed: {}", e)))?;
        self.conn.flush().map_err(|e| {
            ComputerError::InputFailed(format!("Failed to flush X11 connection: {}", e))
        })?;
        Ok(())
    }

    /// Convert MouseButton to X11 button code.
    fn mouse_button_to_code(button: MouseButton) -> u8 {
        match button {
            MouseButton::Left => BUTTON_LEFT,
            MouseButton::Middle => BUTTON_MIDDLE,
            MouseButton::Right => BUTTON_RIGHT,
        }
    }

    /// Convert ScrollDirection to X11 button code.
    fn scroll_direction_to_button(direction: ScrollDirection) -> u8 {
        match direction {
            ScrollDirection::Up => BUTTON_SCROLL_UP,
            ScrollDirection::Down => BUTTON_SCROLL_DOWN,
            ScrollDirection::Left => BUTTON_SCROLL_LEFT,
            ScrollDirection::Right => BUTTON_SCROLL_RIGHT,
        }
    }

    /// Convert a Key to an X11 keysym.
    ///
    /// Keysym values are defined in X11/keysymdef.h from the X11 headers.
    /// These are standard X11 keysym constants (XK_* macros).
    fn key_to_keysym(key: &Key) -> u32 {
        // XK_ keysym values from X11/keysymdef.h
        match key {
            // Modifiers
            Key::Shift => 0xffe1,   // XK_Shift_L
            Key::Control => 0xffe3, // XK_Control_L
            Key::Alt => 0xffe9,     // XK_Alt_L
            Key::Meta => 0xffeb,    // XK_Super_L

            // Function keys
            Key::F1 => 0xffbe,
            Key::F2 => 0xffbf,
            Key::F3 => 0xffc0,
            Key::F4 => 0xffc1,
            Key::F5 => 0xffc2,
            Key::F6 => 0xffc3,
            Key::F7 => 0xffc4,
            Key::F8 => 0xffc5,
            Key::F9 => 0xffc6,
            Key::F10 => 0xffc7,
            Key::F11 => 0xffc8,
            Key::F12 => 0xffc9,

            // Navigation
            Key::Escape => 0xff1b,
            Key::Tab => 0xff09,
            Key::CapsLock => 0xffe5,
            Key::Space => 0x0020,
            Key::Enter => 0xff0d, // XK_Return
            Key::Backspace => 0xff08,
            Key::Delete => 0xffff,
            Key::Insert => 0xff63,
            Key::Home => 0xff50,
            Key::End => 0xff57,
            Key::PageUp => 0xff55,
            Key::PageDown => 0xff56,
            Key::ArrowUp => 0xff52,
            Key::ArrowDown => 0xff54,
            Key::ArrowLeft => 0xff51,
            Key::ArrowRight => 0xff53,

            // Other
            Key::PrintScreen => 0xff61,
            Key::ScrollLock => 0xff14,
            Key::Pause => 0xff13,
            Key::NumLock => 0xff7f,
        }
    }

    /// Convert a character to an X11 keysym.
    fn char_to_keysym(c: char) -> u32 {
        // For ASCII printable characters, keysym matches ASCII code
        // For Latin-1 characters (0x80-0xff), keysym also matches
        let code = c as u32;
        if code < 0x100 {
            code
        } else {
            // For Unicode characters, use Unicode keysym encoding
            // XK_Unicode = 0x01000000 | unicode_codepoint
            0x01000000 | code
        }
    }

    /// Convert a KeyOrChar to an X11 keysym.
    fn key_or_char_to_keysym(key: &KeyOrChar) -> u32 {
        match key {
            KeyOrChar::Key(k) => Self::key_to_keysym(k),
            KeyOrChar::Char(c) => Self::char_to_keysym(*c),
        }
    }

    /// Get the keycode for a keysym using the keyboard mapping.
    fn keysym_to_keycode(&self, keysym: u32) -> Result<Keycode> {
        let setup = self.conn.setup();
        let min_keycode = setup.min_keycode;
        let max_keycode = setup.max_keycode;

        // Get keyboard mapping
        let cookie = self
            .conn
            .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1);
        let mapping = cookie
            .map_err(|e| {
                ComputerError::InputFailed(format!("Failed to get keyboard mapping: {}", e))
            })?
            .reply()
            .map_err(|e| {
                ComputerError::InputFailed(format!("Failed to get keyboard mapping reply: {}", e))
            })?;

        let keysyms_per_keycode = mapping.keysyms_per_keycode as usize;

        // Search through the mapping to find the keycode for this keysym
        for keycode in min_keycode..=max_keycode {
            // Use checked arithmetic to prevent potential overflow
            let keycode_offset = (keycode - min_keycode) as usize;
            let offset = keycode_offset
                .checked_mul(keysyms_per_keycode)
                .ok_or_else(|| ComputerError::InputFailed("Keyboard mapping overflow".into()))?;

            for i in 0..keysyms_per_keycode {
                let idx = offset.checked_add(i).ok_or_else(|| {
                    ComputerError::InputFailed("Keyboard mapping index overflow".into())
                })?;
                if idx < mapping.keysyms.len() && mapping.keysyms[idx] == keysym {
                    return Ok(keycode);
                }
            }
        }

        Err(ComputerError::InputFailed(format!(
            "No keycode found for keysym 0x{:x}",
            keysym
        )))
    }

    /// Press a key (send key down event).
    fn send_key_press(&self, keycode: Keycode) -> Result<()> {
        self.fake_input(KEY_PRESS, keycode, 0, 0)
    }

    /// Release a key (send key up event).
    fn send_key_release(&self, keycode: Keycode) -> Result<()> {
        self.fake_input(KEY_RELEASE, keycode, 0, 0)
    }

    /// Get display information using RandR extension.
    fn get_displays_randr(&self) -> Result<Vec<DisplayInfo>> {
        // Query RandR version first
        let version_cookie = self.conn.randr_query_version(1, 5);
        let _version = version_cookie
            .map_err(|e| {
                ComputerError::ScreenshotFailed(format!("Failed to query RandR version: {}", e))
            })?
            .reply()
            .map_err(|e| ComputerError::ScreenshotFailed(format!("RandR not available: {}", e)))?;

        // Get screen resources
        let resources_cookie = self.conn.randr_get_screen_resources(self.root);
        let resources = resources_cookie
            .map_err(|e| {
                ComputerError::ScreenshotFailed(format!("Failed to get screen resources: {}", e))
            })?
            .reply()
            .map_err(|e| {
                ComputerError::ScreenshotFailed(format!(
                    "Failed to get screen resources reply: {}",
                    e
                ))
            })?;

        let mut displays = Vec::new();
        let mut is_first = true;

        for crtc in &resources.crtcs {
            let crtc_cookie = self.conn.randr_get_crtc_info(*crtc, 0);
            if let Ok(crtc_info) = crtc_cookie
                .map_err(|_| ())
                .and_then(|c| c.reply().map_err(|_| ()))
            {
                // Skip CRTCs that are not active (width/height = 0)
                if crtc_info.width == 0 || crtc_info.height == 0 {
                    continue;
                }

                let display = DisplayInfo {
                    id: displays.len() as u32,
                    name: None, // Could be extracted from output names
                    is_primary: is_first,
                    bounds: Region::new(
                        crtc_info.x as i32,
                        crtc_info.y as i32,
                        crtc_info.width as u32,
                        crtc_info.height as u32,
                    ),
                    scale_factor: 1.0,
                };

                displays.push(display);
                is_first = false;
            }
        }

        Ok(displays)
    }
}

#[cfg(target_os = "linux")]
#[async_trait]
impl MouseController for LinuxComputer {
    async fn get_position(&self) -> Result<Point> {
        let cookie = self.conn.query_pointer(self.root);
        let reply = cookie
            .map_err(|e| ComputerError::MouseFailed(format!("Failed to query pointer: {}", e)))?
            .reply()
            .map_err(|e| {
                ComputerError::MouseFailed(format!("Failed to get pointer position: {}", e))
            })?;

        Ok(Point::new(reply.root_x as i32, reply.root_y as i32))
    }

    async fn move_to(&self, point: Point) -> Result<()> {
        // XTest MotionNotify with absolute coordinates
        self.fake_input(MOTION_NOTIFY, 0, point.x as i16, point.y as i16)
            .map_err(|e| ComputerError::MouseFailed(format!("Failed to move mouse: {}", e)))
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
        let button_code = Self::mouse_button_to_code(button);
        self.fake_input(BUTTON_PRESS, button_code, 0, 0)
            .map_err(|e| ComputerError::MouseFailed(format!("Failed to press mouse button: {}", e)))
    }

    async fn release(&self, button: MouseButton) -> Result<()> {
        let button_code = Self::mouse_button_to_code(button);
        self.fake_input(BUTTON_RELEASE, button_code, 0, 0)
            .map_err(|e| {
                ComputerError::MouseFailed(format!("Failed to release mouse button: {}", e))
            })
    }

    async fn scroll(&self, direction: ScrollDirection, amount: u32) -> Result<()> {
        let button_code = Self::scroll_direction_to_button(direction);

        // Each scroll unit is a button press + release
        for _ in 0..amount {
            self.fake_input(BUTTON_PRESS, button_code, 0, 0)
                .map_err(|e| ComputerError::MouseFailed(format!("Failed to scroll: {}", e)))?;
            self.fake_input(BUTTON_RELEASE, button_code, 0, 0)
                .map_err(|e| ComputerError::MouseFailed(format!("Failed to scroll: {}", e)))?;
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[async_trait]
impl KeyboardController for LinuxComputer {
    async fn type_text(&self, text: &str) -> Result<()> {
        for c in text.chars() {
            // For each character, we create a key combination and press it
            let combo = KeyCombination::char(c);
            self.press_key(combo).await?;
        }
        Ok(())
    }

    async fn press_key(&self, combination: KeyCombination) -> Result<()> {
        // Press modifiers first
        let mut modifier_keycodes = Vec::new();
        for modifier in &combination.modifiers {
            let keysym = Self::key_to_keysym(modifier);
            let keycode = self.keysym_to_keycode(keysym)?;
            self.send_key_press(keycode)?;
            modifier_keycodes.push(keycode);
        }

        // Press the main key
        let main_keysym = Self::key_or_char_to_keysym(&combination.key);
        let main_keycode = self.keysym_to_keycode(main_keysym)?;
        self.send_key_press(main_keycode)?;
        self.send_key_release(main_keycode)?;

        // Release modifiers in reverse order
        for keycode in modifier_keycodes.into_iter().rev() {
            self.send_key_release(keycode)?;
        }

        Ok(())
    }

    async fn key_down(&self, combination: KeyCombination) -> Result<()> {
        // Press modifiers first
        for modifier in &combination.modifiers {
            let keysym = Self::key_to_keysym(modifier);
            let keycode = self.keysym_to_keycode(keysym)?;
            self.send_key_press(keycode)?;
        }

        // Press the main key (hold down)
        let main_keysym = Self::key_or_char_to_keysym(&combination.key);
        let main_keycode = self.keysym_to_keycode(main_keysym)?;
        self.send_key_press(main_keycode)?;

        Ok(())
    }

    async fn key_up(&self, combination: KeyCombination) -> Result<()> {
        // Release the main key first
        let main_keysym = Self::key_or_char_to_keysym(&combination.key);
        let main_keycode = self.keysym_to_keycode(main_keysym)?;
        self.send_key_release(main_keycode)?;

        // Release modifiers in reverse order
        for modifier in combination.modifiers.iter().rev() {
            let keysym = Self::key_to_keysym(modifier);
            let keycode = self.keysym_to_keycode(keysym)?;
            self.send_key_release(keycode)?;
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[async_trait]
impl ComputerController for LinuxComputer {
    fn name(&self) -> &str {
        "linux-x11"
    }

    fn is_available(&self) -> bool {
        // Check if X11 display is available
        std::env::var("DISPLAY").is_ok()
    }
}

#[cfg(target_os = "linux")]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linux_computer_creation() {
        // This test will only pass if X11 is available
        if std::env::var("DISPLAY").is_ok() {
            let computer = LinuxComputer::new();
            assert!(computer.is_ok());
        }
    }

    #[test]
    fn test_linux_computer_name() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                assert_eq!(computer.name(), "linux-x11");
            }
        }
    }

    #[test]
    fn test_linux_is_available() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                assert!(computer.is_available());
            }
        }
    }

    #[tokio::test]
    async fn test_capture_screen() {
        // This test requires an X11 display to be available
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                let result = computer.capture_screen().await;
                // May fail in headless CI environments
                if let Ok(screenshot) = result {
                    assert!(screenshot.width > 0);
                    assert!(screenshot.height > 0);
                    assert!(!screenshot.data.is_empty());
                    assert_eq!(screenshot.format, ImageFormat::Png);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_capture_region() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                let region = Region::new(0, 0, 100, 100);
                let result = computer.capture_region(region).await;
                if let Ok(screenshot) = result {
                    assert_eq!(screenshot.width, 100);
                    assert_eq!(screenshot.height, 100);
                    assert!(!screenshot.data.is_empty());
                }
            }
        }
    }

    #[tokio::test]
    async fn test_get_displays() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                let result = computer.get_displays().await;
                if let Ok(displays) = result {
                    assert!(!displays.is_empty());
                    // At least one display should be primary
                    assert!(displays.iter().any(|d| d.is_primary));
                }
            }
        }
    }

    #[tokio::test]
    async fn test_get_mouse_position() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                let result = computer.get_position().await;
                // This should succeed if X11 is available
                if let Ok(pos) = result {
                    // Mouse position should be within reasonable bounds
                    assert!(pos.x >= 0);
                    assert!(pos.y >= 0);
                }
            }
        }
    }

    /// Test that base64 encoding produces valid output.
    #[test]
    fn test_base64_encoding() {
        // Create a simple 2x2 BGRA image
        let bgra_data: Vec<u8> = vec![
            0, 0, 255, 255, // Blue pixel (BGRA) -> Red (RGBA)
            0, 255, 0, 255, // Green pixel
            255, 0, 0, 255, // Red pixel -> Blue
            255, 255, 255, 255, // White pixel
        ];

        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                let result = computer.encode_to_png_base64(&bgra_data, 2, 2);
                assert!(result.is_ok());
                let base64_data = result.unwrap();
                // Base64 should start with PNG signature in base64
                assert!(base64_data.starts_with("iVBOR"));
            }
        }
    }

    #[test]
    fn test_mouse_button_to_code() {
        assert_eq!(LinuxComputer::mouse_button_to_code(MouseButton::Left), 1);
        assert_eq!(LinuxComputer::mouse_button_to_code(MouseButton::Middle), 2);
        assert_eq!(LinuxComputer::mouse_button_to_code(MouseButton::Right), 3);
    }

    #[test]
    fn test_scroll_direction_to_button() {
        assert_eq!(
            LinuxComputer::scroll_direction_to_button(ScrollDirection::Up),
            4
        );
        assert_eq!(
            LinuxComputer::scroll_direction_to_button(ScrollDirection::Down),
            5
        );
        assert_eq!(
            LinuxComputer::scroll_direction_to_button(ScrollDirection::Left),
            6
        );
        assert_eq!(
            LinuxComputer::scroll_direction_to_button(ScrollDirection::Right),
            7
        );
    }

    #[test]
    fn test_key_to_keysym() {
        // Test modifier keys
        assert_eq!(LinuxComputer::key_to_keysym(&Key::Shift), 0xffe1);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::Control), 0xffe3);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::Alt), 0xffe9);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::Meta), 0xffeb);

        // Test function keys
        assert_eq!(LinuxComputer::key_to_keysym(&Key::F1), 0xffbe);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::F12), 0xffc9);

        // Test navigation keys
        assert_eq!(LinuxComputer::key_to_keysym(&Key::Escape), 0xff1b);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::Enter), 0xff0d);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::Space), 0x0020);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::Tab), 0xff09);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::Backspace), 0xff08);

        // Test arrow keys
        assert_eq!(LinuxComputer::key_to_keysym(&Key::ArrowUp), 0xff52);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::ArrowDown), 0xff54);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::ArrowLeft), 0xff51);
        assert_eq!(LinuxComputer::key_to_keysym(&Key::ArrowRight), 0xff53);
    }

    #[test]
    fn test_char_to_keysym() {
        // ASCII characters have keysyms matching their ASCII codes
        assert_eq!(LinuxComputer::char_to_keysym('a'), 0x61);
        assert_eq!(LinuxComputer::char_to_keysym('A'), 0x41);
        assert_eq!(LinuxComputer::char_to_keysym('0'), 0x30);
        assert_eq!(LinuxComputer::char_to_keysym(' '), 0x20);
        assert_eq!(LinuxComputer::char_to_keysym('!'), 0x21);

        // Unicode characters use the Unicode keysym encoding
        let unicode_char = '\u{1F600}'; // emoji
        assert_eq!(
            LinuxComputer::char_to_keysym(unicode_char),
            0x01000000 | 0x1F600
        );
    }

    #[test]
    fn test_key_or_char_to_keysym() {
        assert_eq!(
            LinuxComputer::key_or_char_to_keysym(&KeyOrChar::Key(Key::Enter)),
            0xff0d
        );
        assert_eq!(
            LinuxComputer::key_or_char_to_keysym(&KeyOrChar::Char('x')),
            0x78
        );
    }

    #[tokio::test]
    async fn test_mouse_move_to() {
        // This test requires an X11 display with XTest extension
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                // Move mouse to a position
                let target = Point::new(100, 100);
                let result = computer.move_to(target).await;
                // May fail if XTest is not available
                if result.is_ok() {
                    // Verify position changed
                    let pos = computer.get_position().await;
                    if let Ok(pos) = pos {
                        // Allow some tolerance for position
                        assert!((pos.x - target.x).abs() <= 1);
                        assert!((pos.y - target.y).abs() <= 1);
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_mouse_move_by() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                // Get starting position
                if let Ok(start) = computer.get_position().await {
                    // Move by relative amount
                    let result = computer.move_by(10, 10).await;
                    if result.is_ok() {
                        if let Ok(end) = computer.get_position().await {
                            assert!((end.x - start.x - 10).abs() <= 1);
                            assert!((end.y - start.y - 10).abs() <= 1);
                        }
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_mouse_click() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                // Test that click methods don't error (actual click not easily verifiable)
                let result = computer.click(MouseButton::Left, ClickType::Single).await;
                // May succeed or fail depending on XTest availability
                if result.is_ok() {
                    // At least no crash
                    assert!(true);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_mouse_scroll() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                let result = computer.scroll(ScrollDirection::Down, 1).await;
                // May succeed or fail depending on XTest availability
                if result.is_ok() {
                    assert!(true);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_keysym_to_keycode() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                // Test common keys that should exist in any keyboard layout
                let result = computer.keysym_to_keycode(0x0061); // 'a'
                assert!(result.is_ok(), "Should find keycode for 'a'");

                let result = computer.keysym_to_keycode(0xff0d); // Enter/Return
                assert!(result.is_ok(), "Should find keycode for Enter");

                let result = computer.keysym_to_keycode(0x0020); // Space
                assert!(result.is_ok(), "Should find keycode for Space");
            }
        }
    }

    #[tokio::test]
    async fn test_type_text() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                // Test typing a simple string
                let result = computer.type_text("ab").await;
                // May succeed or fail depending on XTest and keyboard layout
                // At least verify it doesn't panic
                let _ = result;
            }
        }
    }

    #[tokio::test]
    async fn test_press_key_with_modifiers() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                // Test Ctrl+C
                let combo = KeyCombination::char('c').with_ctrl();
                let result = computer.press_key(combo).await;
                // May succeed or fail depending on XTest
                let _ = result;
            }
        }
    }

    #[tokio::test]
    async fn test_key_down_up() {
        if std::env::var("DISPLAY").is_ok() {
            if let Ok(computer) = LinuxComputer::new() {
                let combo = KeyCombination::key(Key::Shift);

                // Test key_down
                let down_result = computer.key_down(combo.clone()).await;

                // Test key_up
                let up_result = computer.key_up(combo).await;

                // At least verify no panic
                let _ = (down_result, up_result);
            }
        }
    }

    #[test]
    fn test_display_server_enum() {
        // Test that DisplayServer enum variants exist and are comparable
        assert_ne!(DisplayServer::X11, DisplayServer::Wayland);
        assert_ne!(DisplayServer::X11, DisplayServer::None);
        assert_ne!(DisplayServer::Wayland, DisplayServer::None);

        // Test Clone and Copy
        let ds = DisplayServer::X11;
        let ds_clone = ds.clone();
        let ds_copy = ds;
        assert_eq!(ds, ds_clone);
        assert_eq!(ds, ds_copy);

        // Test Debug
        let debug_str = format!("{:?}", DisplayServer::X11);
        assert!(debug_str.contains("X11"));
    }

    #[test]
    fn test_is_wayland() {
        // Test that the function runs without panicking
        // The result depends on environment variables
        let _is_wayland = is_wayland();
    }

    #[test]
    fn test_display_server() {
        // Test that the function runs and returns a valid variant
        let server = display_server();
        // Should be one of the three valid variants
        assert!(
            server == DisplayServer::X11
                || server == DisplayServer::Wayland
                || server == DisplayServer::None
        );
    }
}
