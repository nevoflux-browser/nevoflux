//! Common types for computer use operations.

use serde::{Deserialize, Serialize};

/// Screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    /// X coordinate.
    pub x: i32,
    /// Y coordinate.
    pub y: i32,
}

impl Point {
    /// Create a new point.
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Create a point at origin (0, 0).
    pub fn origin() -> Self {
        Self { x: 0, y: 0 }
    }

    /// Calculate distance to another point.
    pub fn distance_to(&self, other: &Point) -> f64 {
        let dx = (other.x - self.x) as f64;
        let dy = (other.y - self.y) as f64;
        (dx * dx + dy * dy).sqrt()
    }
}

impl From<(i32, i32)> for Point {
    fn from((x, y): (i32, i32)) -> Self {
        Self { x, y }
    }
}

/// Screen region (rectangle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// Top-left X coordinate.
    pub x: i32,
    /// Top-left Y coordinate.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

impl Region {
    /// Create a new region.
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Create a region covering the entire screen (placeholder dimensions).
    pub fn full_screen() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }
    }

    /// Get the top-left corner.
    pub fn top_left(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Get the bottom-right corner.
    pub fn bottom_right(&self) -> Point {
        Point::new(self.x + self.width as i32, self.y + self.height as i32)
    }

    /// Get the center point.
    pub fn center(&self) -> Point {
        Point::new(
            self.x + (self.width / 2) as i32,
            self.y + (self.height / 2) as i32,
        )
    }

    /// Check if a point is within this region.
    pub fn contains(&self, point: &Point) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width as i32
            && point.y >= self.y
            && point.y < self.y + self.height as i32
    }

    /// Get the area in pixels.
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// Mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    /// Left mouse button.
    #[default]
    Left,
    /// Right mouse button.
    Right,
    /// Middle mouse button (scroll wheel click).
    Middle,
}

/// Click action type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickType {
    /// Single click.
    #[default]
    Single,
    /// Double click.
    Double,
    /// Triple click.
    Triple,
}

/// Scroll direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollDirection {
    /// Scroll up.
    Up,
    /// Scroll down.
    Down,
    /// Scroll left.
    Left,
    /// Scroll right.
    Right,
}

/// Special keys (modifiers and function keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    // Modifiers
    Shift,
    Control,
    Alt,
    Meta, // Windows key / Command key

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Navigation
    Escape,
    Tab,
    CapsLock,
    Space,
    Enter,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Other
    PrintScreen,
    ScrollLock,
    Pause,
    NumLock,
}

impl Key {
    /// Check if this is a modifier key.
    pub fn is_modifier(&self) -> bool {
        matches!(self, Key::Shift | Key::Control | Key::Alt | Key::Meta)
    }

    /// Get the key name as a string.
    pub fn name(&self) -> &'static str {
        match self {
            Key::Shift => "Shift",
            Key::Control => "Control",
            Key::Alt => "Alt",
            Key::Meta => "Meta",
            Key::F1 => "F1",
            Key::F2 => "F2",
            Key::F3 => "F3",
            Key::F4 => "F4",
            Key::F5 => "F5",
            Key::F6 => "F6",
            Key::F7 => "F7",
            Key::F8 => "F8",
            Key::F9 => "F9",
            Key::F10 => "F10",
            Key::F11 => "F11",
            Key::F12 => "F12",
            Key::Escape => "Escape",
            Key::Tab => "Tab",
            Key::CapsLock => "CapsLock",
            Key::Space => "Space",
            Key::Enter => "Enter",
            Key::Backspace => "Backspace",
            Key::Delete => "Delete",
            Key::Insert => "Insert",
            Key::Home => "Home",
            Key::End => "End",
            Key::PageUp => "PageUp",
            Key::PageDown => "PageDown",
            Key::ArrowUp => "ArrowUp",
            Key::ArrowDown => "ArrowDown",
            Key::ArrowLeft => "ArrowLeft",
            Key::ArrowRight => "ArrowRight",
            Key::PrintScreen => "PrintScreen",
            Key::ScrollLock => "ScrollLock",
            Key::Pause => "Pause",
            Key::NumLock => "NumLock",
        }
    }
}

/// Key combination (key with modifiers).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyCombination {
    /// The main key (can be a special key or a character).
    pub key: KeyOrChar,
    /// Modifier keys held during the press.
    #[serde(default)]
    pub modifiers: Vec<Key>,
}

impl KeyCombination {
    /// Create a simple key press (no modifiers).
    pub fn key(key: Key) -> Self {
        Self {
            key: KeyOrChar::Key(key),
            modifiers: Vec::new(),
        }
    }

    /// Create a character key press (no modifiers).
    pub fn char(c: char) -> Self {
        Self {
            key: KeyOrChar::Char(c),
            modifiers: Vec::new(),
        }
    }

    /// Add a modifier.
    pub fn with_modifier(mut self, modifier: Key) -> Self {
        if modifier.is_modifier() && !self.modifiers.contains(&modifier) {
            self.modifiers.push(modifier);
        }
        self
    }

    /// Add Shift modifier.
    pub fn with_shift(self) -> Self {
        self.with_modifier(Key::Shift)
    }

    /// Add Control modifier.
    pub fn with_ctrl(self) -> Self {
        self.with_modifier(Key::Control)
    }

    /// Add Alt modifier.
    pub fn with_alt(self) -> Self {
        self.with_modifier(Key::Alt)
    }

    /// Add Meta (Windows/Command) modifier.
    pub fn with_meta(self) -> Self {
        self.with_modifier(Key::Meta)
    }
}

/// Either a special key or a character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyOrChar {
    /// Special key.
    Key(Key),
    /// Character key.
    Char(char),
}

/// Screenshot format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    /// PNG format.
    #[default]
    Png,
    /// JPEG format.
    Jpeg,
}

/// Screenshot result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screenshot {
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
    /// Image format.
    pub format: ImageFormat,
    /// Base64-encoded image data.
    pub data: String,
}

impl Screenshot {
    /// Create a new screenshot.
    pub fn new(width: u32, height: u32, format: ImageFormat, data: String) -> Self {
        Self {
            width,
            height,
            format,
            data,
        }
    }

    /// Get the approximate size in bytes.
    pub fn size_bytes(&self) -> usize {
        self.data.len() * 3 / 4 // Base64 is ~4/3 the size of binary
    }
}

/// Display information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    /// Display ID/index.
    pub id: u32,
    /// Display name.
    pub name: Option<String>,
    /// Whether this is the primary display.
    pub is_primary: bool,
    /// Display bounds.
    pub bounds: Region,
    /// Scale factor (for HiDPI).
    pub scale_factor: f64,
}

impl DisplayInfo {
    /// Create a primary display with default settings.
    pub fn primary(width: u32, height: u32) -> Self {
        Self {
            id: 0,
            name: Some("Primary".to_string()),
            is_primary: true,
            bounds: Region::new(0, 0, width, height),
            scale_factor: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_new() {
        let p = Point::new(100, 200);
        assert_eq!(p.x, 100);
        assert_eq!(p.y, 200);
    }

    #[test]
    fn test_point_origin() {
        let p = Point::origin();
        assert_eq!(p.x, 0);
        assert_eq!(p.y, 0);
    }

    #[test]
    fn test_point_from_tuple() {
        let p: Point = (50, 75).into();
        assert_eq!(p.x, 50);
        assert_eq!(p.y, 75);
    }

    #[test]
    fn test_point_distance() {
        let p1 = Point::new(0, 0);
        let p2 = Point::new(3, 4);
        assert!((p1.distance_to(&p2) - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_region_new() {
        let r = Region::new(10, 20, 100, 200);
        assert_eq!(r.x, 10);
        assert_eq!(r.y, 20);
        assert_eq!(r.width, 100);
        assert_eq!(r.height, 200);
    }

    #[test]
    fn test_region_corners() {
        let r = Region::new(10, 20, 100, 200);
        assert_eq!(r.top_left(), Point::new(10, 20));
        assert_eq!(r.bottom_right(), Point::new(110, 220));
    }

    #[test]
    fn test_region_center() {
        let r = Region::new(0, 0, 100, 200);
        assert_eq!(r.center(), Point::new(50, 100));
    }

    #[test]
    fn test_region_contains() {
        let r = Region::new(10, 10, 100, 100);
        assert!(r.contains(&Point::new(50, 50)));
        assert!(r.contains(&Point::new(10, 10)));
        assert!(!r.contains(&Point::new(5, 5)));
        assert!(!r.contains(&Point::new(110, 110)));
    }

    #[test]
    fn test_region_area() {
        let r = Region::new(0, 0, 100, 200);
        assert_eq!(r.area(), 20000);
    }

    #[test]
    fn test_mouse_button_default() {
        let button = MouseButton::default();
        assert_eq!(button, MouseButton::Left);
    }

    #[test]
    fn test_click_type_default() {
        let click = ClickType::default();
        assert_eq!(click, ClickType::Single);
    }

    #[test]
    fn test_key_is_modifier() {
        assert!(Key::Shift.is_modifier());
        assert!(Key::Control.is_modifier());
        assert!(Key::Alt.is_modifier());
        assert!(Key::Meta.is_modifier());
        assert!(!Key::Enter.is_modifier());
        assert!(!Key::F1.is_modifier());
    }

    #[test]
    fn test_key_name() {
        assert_eq!(Key::Shift.name(), "Shift");
        assert_eq!(Key::Enter.name(), "Enter");
        assert_eq!(Key::F1.name(), "F1");
    }

    #[test]
    fn test_key_combination_simple() {
        let combo = KeyCombination::key(Key::Enter);
        assert_eq!(combo.key, KeyOrChar::Key(Key::Enter));
        assert!(combo.modifiers.is_empty());
    }

    #[test]
    fn test_key_combination_char() {
        let combo = KeyCombination::char('a');
        assert_eq!(combo.key, KeyOrChar::Char('a'));
    }

    #[test]
    fn test_key_combination_with_modifiers() {
        let combo = KeyCombination::char('c').with_ctrl().with_shift();

        assert_eq!(combo.modifiers.len(), 2);
        assert!(combo.modifiers.contains(&Key::Control));
        assert!(combo.modifiers.contains(&Key::Shift));
    }

    #[test]
    fn test_key_combination_no_duplicate_modifiers() {
        let combo = KeyCombination::char('a').with_ctrl().with_ctrl();

        assert_eq!(combo.modifiers.len(), 1);
    }

    #[test]
    fn test_screenshot_new() {
        let ss = Screenshot::new(1920, 1080, ImageFormat::Png, "base64data".to_string());
        assert_eq!(ss.width, 1920);
        assert_eq!(ss.height, 1080);
        assert_eq!(ss.format, ImageFormat::Png);
    }

    #[test]
    fn test_screenshot_size_bytes() {
        let data = "AAAA".repeat(100); // 400 base64 chars = ~300 bytes
        let ss = Screenshot::new(100, 100, ImageFormat::Png, data);
        assert!(ss.size_bytes() > 0);
    }

    #[test]
    fn test_display_info_primary() {
        let display = DisplayInfo::primary(1920, 1080);
        assert!(display.is_primary);
        assert_eq!(display.id, 0);
        assert_eq!(display.bounds.width, 1920);
        assert_eq!(display.bounds.height, 1080);
    }

    #[test]
    fn test_point_serialization() {
        let p = Point::new(100, 200);
        let json = serde_json::to_string(&p).unwrap();
        let decoded: Point = serde_json::from_str(&json).unwrap();
        assert_eq!(p, decoded);
    }

    #[test]
    fn test_region_serialization() {
        let r = Region::new(10, 20, 100, 200);
        let json = serde_json::to_string(&r).unwrap();
        let decoded: Region = serde_json::from_str(&json).unwrap();
        assert_eq!(r, decoded);
    }

    #[test]
    fn test_mouse_button_serialization() {
        let buttons = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];
        for button in buttons {
            let json = serde_json::to_string(&button).unwrap();
            let decoded: MouseButton = serde_json::from_str(&json).unwrap();
            assert_eq!(button, decoded);
        }
    }

    #[test]
    fn test_key_serialization() {
        let key = Key::Control;
        let json = serde_json::to_string(&key).unwrap();
        let decoded: Key = serde_json::from_str(&json).unwrap();
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_key_combination_serialization() {
        let combo = KeyCombination::char('v').with_ctrl();
        let json = serde_json::to_string(&combo).unwrap();
        let decoded: KeyCombination = serde_json::from_str(&json).unwrap();
        assert_eq!(combo, decoded);
    }
}
