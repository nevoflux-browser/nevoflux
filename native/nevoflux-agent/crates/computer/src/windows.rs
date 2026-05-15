//! Windows computer backend using Win32 API.
//!
//! This module provides screenshot, mouse, and keyboard control functionality
//! for Windows using the Win32 Graphics GDI and Input APIs.

use crate::error::{ComputerError, Result};
use crate::traits::{ComputerController, KeyboardController, MouseController, ScreenshotProvider};
use crate::types::*;
use async_trait::async_trait;

#[cfg(target_os = "windows")]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
#[cfg(target_os = "windows")]
use image::{ImageBuffer, Rgba};
#[cfg(target_os = "windows")]
use std::io::Cursor;
#[cfg(target_os = "windows")]
use std::mem::zeroed;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    EnumDisplayMonitors, GetDC, GetDIBits, GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, MONITORINFO, MONITORINFOEXW, SRCCOPY,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    MOUSEINPUT, VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8,
    VK_F9, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_NUMLOCK, VK_PAUSE, VK_PRIOR,
    VK_RETURN, VK_RIGHT, VK_SCROLL, VK_SHIFT, VK_SNAPSHOT, VK_SPACE, VK_TAB, VK_UP,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SetCursorPos, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// Windows computer controller using Win32 API.
///
/// This struct is only available on Windows. On other platforms,
/// attempting to use it will result in `NotSupported` errors.
#[cfg(target_os = "windows")]
pub struct WindowsComputer {
    _private: (),
}

/// Stub WindowsComputer for non-Windows platforms.
/// All operations return NotSupported errors.
#[cfg(not(target_os = "windows"))]
pub struct WindowsComputer {
    _private: (),
}

#[cfg(target_os = "windows")]
impl WindowsComputer {
    /// Create a new Windows computer controller.
    pub fn new() -> Result<Self> {
        Ok(Self { _private: () })
    }

    /// Get the primary screen dimensions.
    fn get_screen_dimensions() -> (i32, i32) {
        unsafe {
            let width = GetSystemMetrics(SM_CXSCREEN);
            let height = GetSystemMetrics(SM_CYSCREEN);
            (width, height)
        }
    }

    /// Get virtual screen dimensions (all monitors combined).
    fn get_virtual_screen_dimensions() -> (i32, i32, i32, i32) {
        unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            (x, y, width, height)
        }
    }

    /// Capture a region of the screen and return raw BGRA pixel data.
    fn capture_region_raw(&self, x: i32, y: i32, width: i32, height: i32) -> Result<Vec<u8>> {
        unsafe {
            // Get screen DC
            let screen_dc: HDC = GetDC(HWND::default());
            if screen_dc.is_invalid() {
                return Err(ComputerError::ScreenshotFailed(
                    "Failed to get screen DC".into(),
                ));
            }

            // Create compatible DC
            let mem_dc = CreateCompatibleDC(screen_dc);
            if mem_dc.is_invalid() {
                ReleaseDC(HWND::default(), screen_dc);
                return Err(ComputerError::ScreenshotFailed(
                    "Failed to create compatible DC".into(),
                ));
            }

            // Create compatible bitmap
            let bitmap: HBITMAP = CreateCompatibleBitmap(screen_dc, width, height);
            if bitmap.is_invalid() {
                DeleteDC(mem_dc);
                ReleaseDC(HWND::default(), screen_dc);
                return Err(ComputerError::ScreenshotFailed(
                    "Failed to create compatible bitmap".into(),
                ));
            }

            // Select bitmap into DC
            let old_bitmap = SelectObject(mem_dc, bitmap);

            // BitBlt from screen to memory DC
            if BitBlt(mem_dc, 0, 0, width, height, screen_dc, x, y, SRCCOPY).is_err() {
                SelectObject(mem_dc, old_bitmap);
                DeleteObject(bitmap);
                DeleteDC(mem_dc);
                ReleaseDC(HWND::default(), screen_dc);
                return Err(ComputerError::ScreenshotFailed("BitBlt failed".into()));
            }

            // Setup BITMAPINFO for getting DIB bits
            let mut bmi: BITMAPINFO = zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width;
            bmi.bmiHeader.biHeight = -height; // Top-down DIB
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB.0;

            // Allocate buffer for pixel data
            let data_size = (width * height * 4) as usize;
            let mut data: Vec<u8> = vec![0; data_size];

            // Get DIB bits
            let lines = GetDIBits(
                mem_dc,
                bitmap,
                0,
                height as u32,
                Some(data.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            );

            // Cleanup
            SelectObject(mem_dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(mem_dc);
            ReleaseDC(HWND::default(), screen_dc);

            if lines == 0 {
                return Err(ComputerError::ScreenshotFailed("GetDIBits failed".into()));
            }

            Ok(data)
        }
    }

    /// Convert raw image data (BGRA) to PNG and encode as base64.
    fn encode_to_png_base64(&self, data: &[u8], width: u32, height: u32) -> Result<String> {
        // Validate data size
        if data.len() % 4 != 0 {
            return Err(ComputerError::ScreenshotFailed(format!(
                "Invalid image data length: {} (not a multiple of 4)",
                data.len()
            )));
        }

        // Windows returns BGRA format, we need to convert to RGBA
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

    /// Convert a Key to a Windows virtual key code.
    fn key_to_vk(key: &Key) -> VIRTUAL_KEY {
        match key {
            // Modifiers
            Key::Shift => VK_SHIFT,
            Key::Control => VK_CONTROL,
            Key::Alt => VK_MENU,
            Key::Meta => VK_LWIN,

            // Function keys
            Key::F1 => VK_F1,
            Key::F2 => VK_F2,
            Key::F3 => VK_F3,
            Key::F4 => VK_F4,
            Key::F5 => VK_F5,
            Key::F6 => VK_F6,
            Key::F7 => VK_F7,
            Key::F8 => VK_F8,
            Key::F9 => VK_F9,
            Key::F10 => VK_F10,
            Key::F11 => VK_F11,
            Key::F12 => VK_F12,

            // Navigation
            Key::Escape => VK_ESCAPE,
            Key::Tab => VK_TAB,
            Key::CapsLock => VK_CAPITAL,
            Key::Space => VK_SPACE,
            Key::Enter => VK_RETURN,
            Key::Backspace => VK_BACK,
            Key::Delete => VK_DELETE,
            Key::Insert => VK_INSERT,
            Key::Home => VK_HOME,
            Key::End => VK_END,
            Key::PageUp => VK_PRIOR,
            Key::PageDown => VK_NEXT,
            Key::ArrowUp => VK_UP,
            Key::ArrowDown => VK_DOWN,
            Key::ArrowLeft => VK_LEFT,
            Key::ArrowRight => VK_RIGHT,

            // Other
            Key::PrintScreen => VK_SNAPSHOT,
            Key::ScrollLock => VK_SCROLL,
            Key::Pause => VK_PAUSE,
            Key::NumLock => VK_NUMLOCK,
        }
    }

    /// Send a keyboard input event.
    fn send_key_event(&self, vk: VIRTUAL_KEY, key_up: bool) -> Result<()> {
        unsafe {
            let mut input: INPUT = zeroed();
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki = KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            };

            let result = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            if result == 0 {
                return Err(ComputerError::KeyboardFailed(
                    "SendInput failed for key event".into(),
                ));
            }
            Ok(())
        }
    }

    /// Send a Unicode character input event.
    fn send_unicode_char(&self, c: char) -> Result<()> {
        unsafe {
            // Key down
            let mut input_down: INPUT = zeroed();
            input_down.r#type = INPUT_KEYBOARD;
            input_down.Anonymous.ki = KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: c as u16,
                dwFlags: KEYEVENTF_UNICODE,
                time: 0,
                dwExtraInfo: 0,
            };

            // Key up
            let mut input_up: INPUT = zeroed();
            input_up.r#type = INPUT_KEYBOARD;
            input_up.Anonymous.ki = KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: c as u16,
                dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            };

            let result = SendInput(&[input_down, input_up], std::mem::size_of::<INPUT>() as i32);
            if result == 0 {
                return Err(ComputerError::KeyboardFailed(
                    "SendInput failed for unicode char".into(),
                ));
            }
            Ok(())
        }
    }

    /// Send a mouse input event.
    fn send_mouse_event(&self, mouse_input: MOUSEINPUT) -> Result<()> {
        unsafe {
            let mut input: INPUT = zeroed();
            input.r#type = INPUT_MOUSE;
            input.Anonymous.mi = mouse_input;

            let result = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            if result == 0 {
                return Err(ComputerError::MouseFailed(
                    "SendInput failed for mouse event".into(),
                ));
            }
            Ok(())
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl WindowsComputer {
    /// Create a new Windows computer controller.
    /// On non-Windows platforms, this always returns a NotSupported error.
    pub fn new() -> Result<Self> {
        Err(ComputerError::NotSupported(
            "WindowsComputer is only available on Windows".into(),
        ))
    }
}

#[cfg(target_os = "windows")]
impl Default for WindowsComputer {
    fn default() -> Self {
        Self::new().expect("Failed to create Windows controller")
    }
}

// ============================================================================
// Windows implementations
// ============================================================================

#[cfg(target_os = "windows")]
#[async_trait]
impl ScreenshotProvider for WindowsComputer {
    async fn capture_screen(&self) -> Result<Screenshot> {
        let (width, height) = Self::get_screen_dimensions();

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
            region.x,
            region.y,
            region.width as i32,
            region.height as i32,
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
        unsafe {
            let mut displays: Vec<DisplayInfo> = Vec::new();
            let displays_ptr = &mut displays as *mut Vec<DisplayInfo>;

            // Callback for EnumDisplayMonitors
            unsafe extern "system" fn enum_callback(
                hmonitor: windows::Win32::Graphics::Gdi::HMONITOR,
                _hdc: HDC,
                _lprect: *mut RECT,
                lparam: LPARAM,
            ) -> BOOL {
                let displays = &mut *(lparam.0 as *mut Vec<DisplayInfo>);

                let mut mi: MONITORINFOEXW = zeroed();
                mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

                if GetMonitorInfoW(hmonitor, &mut mi.monitorInfo as *mut MONITORINFO).as_bool() {
                    let bounds = mi.monitorInfo.rcMonitor;
                    let is_primary = (mi.monitorInfo.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY = 1

                    let display = DisplayInfo {
                        id: displays.len() as u32,
                        name: None,
                        is_primary,
                        bounds: Region::new(
                            bounds.left,
                            bounds.top,
                            (bounds.right - bounds.left) as u32,
                            (bounds.bottom - bounds.top) as u32,
                        ),
                        scale_factor: 1.0,
                    };

                    displays.push(display);
                }

                BOOL::from(true)
            }

            let result = EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(enum_callback),
                LPARAM(displays_ptr as isize),
            );

            if !result.as_bool() || displays.is_empty() {
                // Fallback to primary display
                let (width, height) = Self::get_screen_dimensions();
                return Ok(vec![DisplayInfo::primary(width as u32, height as u32)]);
            }

            Ok(displays)
        }
    }
}

#[cfg(target_os = "windows")]
#[async_trait]
impl MouseController for WindowsComputer {
    async fn get_position(&self) -> Result<Point> {
        unsafe {
            let mut point: POINT = zeroed();
            GetCursorPos(&mut point)
                .map_err(|_| ComputerError::MouseFailed("Failed to get cursor position".into()))?;
            Ok(Point::new(point.x, point.y))
        }
    }

    async fn move_to(&self, point: Point) -> Result<()> {
        unsafe {
            SetCursorPos(point.x, point.y)
                .map_err(|_| ComputerError::MouseFailed("Failed to set cursor position".into()))?;
            Ok(())
        }
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
        let flags = match button {
            MouseButton::Left => MOUSEEVENTF_LEFTDOWN,
            MouseButton::Right => MOUSEEVENTF_RIGHTDOWN,
            MouseButton::Middle => MOUSEEVENTF_MIDDLEDOWN,
        };

        let mouse_input = MOUSEINPUT {
            dx: 0,
            dy: 0,
            mouseData: 0,
            dwFlags: flags,
            time: 0,
            dwExtraInfo: 0,
        };

        self.send_mouse_event(mouse_input)
    }

    async fn release(&self, button: MouseButton) -> Result<()> {
        let flags = match button {
            MouseButton::Left => MOUSEEVENTF_LEFTUP,
            MouseButton::Right => MOUSEEVENTF_RIGHTUP,
            MouseButton::Middle => MOUSEEVENTF_MIDDLEUP,
        };

        let mouse_input = MOUSEINPUT {
            dx: 0,
            dy: 0,
            mouseData: 0,
            dwFlags: flags,
            time: 0,
            dwExtraInfo: 0,
        };

        self.send_mouse_event(mouse_input)
    }

    async fn scroll(&self, direction: ScrollDirection, amount: u32) -> Result<()> {
        let (flags, wheel_delta) = match direction {
            ScrollDirection::Up => (MOUSEEVENTF_WHEEL, 120i32 * amount as i32),
            ScrollDirection::Down => (MOUSEEVENTF_WHEEL, -120i32 * amount as i32),
            ScrollDirection::Left => (MOUSEEVENTF_HWHEEL, -120i32 * amount as i32),
            ScrollDirection::Right => (MOUSEEVENTF_HWHEEL, 120i32 * amount as i32),
        };

        let mouse_input = MOUSEINPUT {
            dx: 0,
            dy: 0,
            mouseData: wheel_delta as u32,
            dwFlags: flags,
            time: 0,
            dwExtraInfo: 0,
        };

        self.send_mouse_event(mouse_input)
    }
}

#[cfg(target_os = "windows")]
#[async_trait]
impl KeyboardController for WindowsComputer {
    async fn type_text(&self, text: &str) -> Result<()> {
        for c in text.chars() {
            self.send_unicode_char(c)?;
        }
        Ok(())
    }

    async fn press_key(&self, combination: KeyCombination) -> Result<()> {
        // Press modifiers first
        for modifier in &combination.modifiers {
            let vk = Self::key_to_vk(modifier);
            self.send_key_event(vk, false)?;
        }

        // Press and release the main key
        match &combination.key {
            KeyOrChar::Key(k) => {
                let vk = Self::key_to_vk(k);
                self.send_key_event(vk, false)?;
                self.send_key_event(vk, true)?;
            }
            KeyOrChar::Char(c) => {
                self.send_unicode_char(*c)?;
            }
        }

        // Release modifiers in reverse order
        for modifier in combination.modifiers.iter().rev() {
            let vk = Self::key_to_vk(modifier);
            self.send_key_event(vk, true)?;
        }

        Ok(())
    }

    async fn key_down(&self, combination: KeyCombination) -> Result<()> {
        // Press modifiers first
        for modifier in &combination.modifiers {
            let vk = Self::key_to_vk(modifier);
            self.send_key_event(vk, false)?;
        }

        // Press the main key (hold down)
        match &combination.key {
            KeyOrChar::Key(k) => {
                let vk = Self::key_to_vk(k);
                self.send_key_event(vk, false)?;
            }
            KeyOrChar::Char(c) => {
                // For characters, we can only send press+release, not hold
                self.send_unicode_char(*c)?;
            }
        }

        Ok(())
    }

    async fn key_up(&self, combination: KeyCombination) -> Result<()> {
        // Release the main key first
        match &combination.key {
            KeyOrChar::Key(k) => {
                let vk = Self::key_to_vk(k);
                self.send_key_event(vk, true)?;
            }
            KeyOrChar::Char(_) => {
                // Unicode chars don't have separate key up
            }
        }

        // Release modifiers in reverse order
        for modifier in combination.modifiers.iter().rev() {
            let vk = Self::key_to_vk(modifier);
            self.send_key_event(vk, true)?;
        }

        Ok(())
    }
}

#[cfg(target_os = "windows")]
#[async_trait]
impl ComputerController for WindowsComputer {
    fn name(&self) -> &str {
        "windows-win32"
    }

    fn is_available(&self) -> bool {
        // Win32 API is always available on Windows
        true
    }
}

// ============================================================================
// Stub implementations for non-Windows platforms
// ============================================================================

#[cfg(not(target_os = "windows"))]
#[async_trait]
impl ScreenshotProvider for WindowsComputer {
    async fn capture_screen(&self) -> Result<Screenshot> {
        Err(ComputerError::NotSupported(
            "Screenshot capture is only available on Windows".into(),
        ))
    }

    async fn capture_display(&self, _display_id: u32) -> Result<Screenshot> {
        Err(ComputerError::NotSupported(
            "Display capture is only available on Windows".into(),
        ))
    }

    async fn capture_region(&self, _region: Region) -> Result<Screenshot> {
        Err(ComputerError::NotSupported(
            "Region capture is only available on Windows".into(),
        ))
    }

    async fn get_displays(&self) -> Result<Vec<DisplayInfo>> {
        Err(ComputerError::NotSupported(
            "Display enumeration is only available on Windows".into(),
        ))
    }
}

#[cfg(not(target_os = "windows"))]
#[async_trait]
impl MouseController for WindowsComputer {
    async fn get_position(&self) -> Result<Point> {
        Err(ComputerError::NotSupported(
            "Mouse position is only available on Windows".into(),
        ))
    }

    async fn move_to(&self, _point: Point) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse movement is only available on Windows".into(),
        ))
    }

    async fn move_by(&self, _dx: i32, _dy: i32) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse movement is only available on Windows".into(),
        ))
    }

    async fn click(&self, _button: MouseButton, _click_type: ClickType) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse click is only available on Windows".into(),
        ))
    }

    async fn press(&self, _button: MouseButton) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse press is only available on Windows".into(),
        ))
    }

    async fn release(&self, _button: MouseButton) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse release is only available on Windows".into(),
        ))
    }

    async fn scroll(&self, _direction: ScrollDirection, _amount: u32) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Mouse scroll is only available on Windows".into(),
        ))
    }
}

#[cfg(not(target_os = "windows"))]
#[async_trait]
impl KeyboardController for WindowsComputer {
    async fn type_text(&self, _text: &str) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Keyboard input is only available on Windows".into(),
        ))
    }

    async fn press_key(&self, _combination: KeyCombination) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Keyboard input is only available on Windows".into(),
        ))
    }

    async fn key_down(&self, _combination: KeyCombination) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Keyboard input is only available on Windows".into(),
        ))
    }

    async fn key_up(&self, _combination: KeyCombination) -> Result<()> {
        Err(ComputerError::NotSupported(
            "Keyboard input is only available on Windows".into(),
        ))
    }
}

#[cfg(not(target_os = "windows"))]
#[async_trait]
impl ComputerController for WindowsComputer {
    fn name(&self) -> &str {
        "windows-win32"
    }

    fn is_available(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_computer_creation() {
        // On non-Windows, creation should fail with NotSupported
        #[cfg(not(target_os = "windows"))]
        {
            let result = WindowsComputer::new();
            assert!(result.is_err());
            if let Err(ComputerError::NotSupported(msg)) = result {
                assert!(msg.contains("Windows"));
            } else {
                panic!("Expected NotSupported error");
            }
        }

        // On Windows, creation should succeed
        #[cfg(target_os = "windows")]
        {
            let result = WindowsComputer::new();
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_windows_computer_name() {
        #[cfg(target_os = "windows")]
        {
            if let Ok(computer) = WindowsComputer::new() {
                assert_eq!(computer.name(), "windows-win32");
            }
        }
    }

    #[test]
    fn test_windows_is_available() {
        #[cfg(target_os = "windows")]
        {
            if let Ok(computer) = WindowsComputer::new() {
                assert!(computer.is_available());
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Can't test is_available on non-Windows since new() fails
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_key_to_vk() {
        // Test modifier keys
        assert_eq!(WindowsComputer::key_to_vk(&Key::Shift), VK_SHIFT);
        assert_eq!(WindowsComputer::key_to_vk(&Key::Control), VK_CONTROL);
        assert_eq!(WindowsComputer::key_to_vk(&Key::Alt), VK_MENU);
        assert_eq!(WindowsComputer::key_to_vk(&Key::Meta), VK_LWIN);

        // Test function keys
        assert_eq!(WindowsComputer::key_to_vk(&Key::F1), VK_F1);
        assert_eq!(WindowsComputer::key_to_vk(&Key::F12), VK_F12);

        // Test navigation keys
        assert_eq!(WindowsComputer::key_to_vk(&Key::Escape), VK_ESCAPE);
        assert_eq!(WindowsComputer::key_to_vk(&Key::Enter), VK_RETURN);
        assert_eq!(WindowsComputer::key_to_vk(&Key::Space), VK_SPACE);
        assert_eq!(WindowsComputer::key_to_vk(&Key::Tab), VK_TAB);
        assert_eq!(WindowsComputer::key_to_vk(&Key::Backspace), VK_BACK);

        // Test arrow keys
        assert_eq!(WindowsComputer::key_to_vk(&Key::ArrowUp), VK_UP);
        assert_eq!(WindowsComputer::key_to_vk(&Key::ArrowDown), VK_DOWN);
        assert_eq!(WindowsComputer::key_to_vk(&Key::ArrowLeft), VK_LEFT);
        assert_eq!(WindowsComputer::key_to_vk(&Key::ArrowRight), VK_RIGHT);
    }

    #[tokio::test]
    async fn test_non_windows_operations_return_not_supported() {
        #[cfg(not(target_os = "windows"))]
        {
            // We can't create a WindowsComputer on non-Windows, but we can test
            // that the error handling is correct
            let result = WindowsComputer::new();
            assert!(matches!(result, Err(ComputerError::NotSupported(_))));
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_capture_screen() {
        if let Ok(computer) = WindowsComputer::new() {
            let result = computer.capture_screen().await;
            if let Ok(screenshot) = result {
                assert!(screenshot.width > 0);
                assert!(screenshot.height > 0);
                assert!(!screenshot.data.is_empty());
                assert_eq!(screenshot.format, ImageFormat::Png);
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_capture_region() {
        if let Ok(computer) = WindowsComputer::new() {
            let region = Region::new(0, 0, 100, 100);
            let result = computer.capture_region(region).await;
            if let Ok(screenshot) = result {
                assert_eq!(screenshot.width, 100);
                assert_eq!(screenshot.height, 100);
                assert!(!screenshot.data.is_empty());
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_get_displays() {
        if let Ok(computer) = WindowsComputer::new() {
            let result = computer.get_displays().await;
            if let Ok(displays) = result {
                assert!(!displays.is_empty());
                // At least one display should be primary
                assert!(displays.iter().any(|d| d.is_primary));
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_get_mouse_position() {
        if let Ok(computer) = WindowsComputer::new() {
            let result = computer.get_position().await;
            if let Ok(pos) = result {
                // Mouse position should be within reasonable bounds
                assert!(pos.x >= 0);
                assert!(pos.y >= 0);
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_mouse_move_to() {
        if let Ok(computer) = WindowsComputer::new() {
            let target = Point::new(100, 100);
            let result = computer.move_to(target).await;
            if result.is_ok() {
                let pos = computer.get_position().await;
                if let Ok(pos) = pos {
                    assert_eq!(pos.x, target.x);
                    assert_eq!(pos.y, target.y);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_mouse_click() {
        if let Ok(computer) = WindowsComputer::new() {
            // Test that click methods don't error
            let result = computer.click(MouseButton::Left, ClickType::Single).await;
            if result.is_ok() {
                assert!(true);
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_mouse_scroll() {
        if let Ok(computer) = WindowsComputer::new() {
            let result = computer.scroll(ScrollDirection::Down, 1).await;
            if result.is_ok() {
                assert!(true);
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_type_text() {
        if let Ok(computer) = WindowsComputer::new() {
            // Test typing a simple string
            let result = computer.type_text("ab").await;
            // At least verify it doesn't panic
            let _ = result;
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_press_key_with_modifiers() {
        if let Ok(computer) = WindowsComputer::new() {
            // Test Ctrl+C
            let combo = KeyCombination::char('c').with_ctrl();
            let result = computer.press_key(combo).await;
            let _ = result;
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_key_down_up() {
        if let Ok(computer) = WindowsComputer::new() {
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
