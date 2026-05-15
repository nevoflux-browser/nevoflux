//! Traits for computer use operations.
//!
//! These traits define the interface for screenshot, mouse, and keyboard operations.
//! They can be implemented by real platform-specific backends or mock implementations.

use crate::error::Result;
use crate::types::{
    ClickType, DisplayInfo, KeyCombination, MouseButton, Point, Region, Screenshot, ScrollDirection,
};
use async_trait::async_trait;

/// Trait for screenshot operations.
#[async_trait]
pub trait ScreenshotProvider: Send + Sync {
    /// Capture the entire screen.
    async fn capture_screen(&self) -> Result<Screenshot>;

    /// Capture a specific display by ID.
    async fn capture_display(&self, display_id: u32) -> Result<Screenshot>;

    /// Capture a specific region.
    async fn capture_region(&self, region: Region) -> Result<Screenshot>;

    /// Get information about all displays.
    async fn get_displays(&self) -> Result<Vec<DisplayInfo>>;

    /// Get the primary display info.
    async fn get_primary_display(&self) -> Result<DisplayInfo> {
        let displays = self.get_displays().await?;
        displays.into_iter().find(|d| d.is_primary).ok_or_else(|| {
            crate::error::ComputerError::ScreenshotFailed("No primary display".into())
        })
    }
}

/// Trait for mouse operations.
#[async_trait]
pub trait MouseController: Send + Sync {
    /// Get the current mouse position.
    async fn get_position(&self) -> Result<Point>;

    /// Move the mouse to an absolute position.
    async fn move_to(&self, point: Point) -> Result<()>;

    /// Move the mouse by a relative offset.
    async fn move_by(&self, dx: i32, dy: i32) -> Result<()>;

    /// Click at the current position.
    async fn click(&self, button: MouseButton, click_type: ClickType) -> Result<()>;

    /// Click at a specific position.
    async fn click_at(
        &self,
        point: Point,
        button: MouseButton,
        click_type: ClickType,
    ) -> Result<()> {
        self.move_to(point).await?;
        self.click(button, click_type).await
    }

    /// Press a mouse button (hold down).
    async fn press(&self, button: MouseButton) -> Result<()>;

    /// Release a mouse button.
    async fn release(&self, button: MouseButton) -> Result<()>;

    /// Scroll the mouse wheel.
    async fn scroll(&self, direction: ScrollDirection, amount: u32) -> Result<()>;

    /// Drag from one point to another.
    async fn drag(&self, from: Point, to: Point, button: MouseButton) -> Result<()> {
        self.move_to(from).await?;
        self.press(button).await?;
        self.move_to(to).await?;
        self.release(button).await
    }
}

/// Trait for keyboard operations.
#[async_trait]
pub trait KeyboardController: Send + Sync {
    /// Type a string of text.
    async fn type_text(&self, text: &str) -> Result<()>;

    /// Press a key combination.
    async fn press_key(&self, combination: KeyCombination) -> Result<()>;

    /// Press and hold a key.
    async fn key_down(&self, combination: KeyCombination) -> Result<()>;

    /// Release a key.
    async fn key_up(&self, combination: KeyCombination) -> Result<()>;
}

/// Combined trait for full computer control.
#[async_trait]
pub trait ComputerController: ScreenshotProvider + MouseController + KeyboardController {
    /// Get a human-readable name for this controller.
    fn name(&self) -> &str;

    /// Check if the controller is available on this system.
    fn is_available(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ImageFormat, Key};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Mock screenshot provider for testing.
    struct MockScreenshotProvider {
        capture_count: AtomicU32,
    }

    impl MockScreenshotProvider {
        fn new() -> Self {
            Self {
                capture_count: AtomicU32::new(0),
            }
        }
    }

    #[async_trait]
    impl ScreenshotProvider for MockScreenshotProvider {
        async fn capture_screen(&self) -> Result<Screenshot> {
            self.capture_count.fetch_add(1, Ordering::SeqCst);
            Ok(Screenshot::new(
                1920,
                1080,
                ImageFormat::Png,
                "mock_data".to_string(),
            ))
        }

        async fn capture_display(&self, _display_id: u32) -> Result<Screenshot> {
            self.capture_screen().await
        }

        async fn capture_region(&self, region: Region) -> Result<Screenshot> {
            self.capture_count.fetch_add(1, Ordering::SeqCst);
            Ok(Screenshot::new(
                region.width,
                region.height,
                ImageFormat::Png,
                "mock_region".to_string(),
            ))
        }

        async fn get_displays(&self) -> Result<Vec<DisplayInfo>> {
            Ok(vec![DisplayInfo::primary(1920, 1080)])
        }
    }

    /// Mock mouse controller for testing.
    struct MockMouseController {
        position: tokio::sync::RwLock<Point>,
    }

    impl MockMouseController {
        fn new() -> Self {
            Self {
                position: tokio::sync::RwLock::new(Point::origin()),
            }
        }
    }

    #[async_trait]
    impl MouseController for MockMouseController {
        async fn get_position(&self) -> Result<Point> {
            Ok(*self.position.read().await)
        }

        async fn move_to(&self, point: Point) -> Result<()> {
            *self.position.write().await = point;
            Ok(())
        }

        async fn move_by(&self, dx: i32, dy: i32) -> Result<()> {
            let mut pos = self.position.write().await;
            pos.x += dx;
            pos.y += dy;
            Ok(())
        }

        async fn click(&self, _button: MouseButton, _click_type: ClickType) -> Result<()> {
            Ok(())
        }

        async fn press(&self, _button: MouseButton) -> Result<()> {
            Ok(())
        }

        async fn release(&self, _button: MouseButton) -> Result<()> {
            Ok(())
        }

        async fn scroll(&self, _direction: ScrollDirection, _amount: u32) -> Result<()> {
            Ok(())
        }
    }

    /// Mock keyboard controller for testing.
    struct MockKeyboardController {
        typed_text: tokio::sync::RwLock<String>,
    }

    impl MockKeyboardController {
        fn new() -> Self {
            Self {
                typed_text: tokio::sync::RwLock::new(String::new()),
            }
        }

        async fn get_typed_text(&self) -> String {
            self.typed_text.read().await.clone()
        }
    }

    #[async_trait]
    impl KeyboardController for MockKeyboardController {
        async fn type_text(&self, text: &str) -> Result<()> {
            self.typed_text.write().await.push_str(text);
            Ok(())
        }

        async fn press_key(&self, _combination: KeyCombination) -> Result<()> {
            Ok(())
        }

        async fn key_down(&self, _combination: KeyCombination) -> Result<()> {
            Ok(())
        }

        async fn key_up(&self, _combination: KeyCombination) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_mock_screenshot_provider() {
        let provider = MockScreenshotProvider::new();

        let ss = provider.capture_screen().await.unwrap();
        assert_eq!(ss.width, 1920);
        assert_eq!(ss.height, 1080);
        assert_eq!(provider.capture_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_mock_screenshot_region() {
        let provider = MockScreenshotProvider::new();

        let region = Region::new(0, 0, 100, 100);
        let ss = provider.capture_region(region).await.unwrap();
        assert_eq!(ss.width, 100);
        assert_eq!(ss.height, 100);
    }

    #[tokio::test]
    async fn test_mock_screenshot_displays() {
        let provider = MockScreenshotProvider::new();

        let displays = provider.get_displays().await.unwrap();
        assert_eq!(displays.len(), 1);
        assert!(displays[0].is_primary);
    }

    #[tokio::test]
    async fn test_mock_screenshot_primary_display() {
        let provider = MockScreenshotProvider::new();

        let primary = provider.get_primary_display().await.unwrap();
        assert!(primary.is_primary);
        assert_eq!(primary.bounds.width, 1920);
    }

    #[tokio::test]
    async fn test_mock_mouse_position() {
        let mouse = MockMouseController::new();

        let pos = mouse.get_position().await.unwrap();
        assert_eq!(pos, Point::origin());

        mouse.move_to(Point::new(100, 200)).await.unwrap();
        let pos = mouse.get_position().await.unwrap();
        assert_eq!(pos, Point::new(100, 200));
    }

    #[tokio::test]
    async fn test_mock_mouse_move_by() {
        let mouse = MockMouseController::new();

        mouse.move_to(Point::new(100, 100)).await.unwrap();
        mouse.move_by(50, -25).await.unwrap();

        let pos = mouse.get_position().await.unwrap();
        assert_eq!(pos, Point::new(150, 75));
    }

    #[tokio::test]
    async fn test_mock_mouse_click() {
        let mouse = MockMouseController::new();

        // Should not error
        mouse
            .click(MouseButton::Left, ClickType::Single)
            .await
            .unwrap();
        mouse
            .click(MouseButton::Right, ClickType::Double)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_mock_mouse_click_at() {
        let mouse = MockMouseController::new();

        mouse
            .click_at(Point::new(500, 500), MouseButton::Left, ClickType::Single)
            .await
            .unwrap();

        let pos = mouse.get_position().await.unwrap();
        assert_eq!(pos, Point::new(500, 500));
    }

    #[tokio::test]
    async fn test_mock_mouse_drag() {
        let mouse = MockMouseController::new();

        mouse
            .drag(
                Point::new(100, 100),
                Point::new(200, 200),
                MouseButton::Left,
            )
            .await
            .unwrap();

        let pos = mouse.get_position().await.unwrap();
        assert_eq!(pos, Point::new(200, 200));
    }

    #[tokio::test]
    async fn test_mock_mouse_scroll() {
        let mouse = MockMouseController::new();

        mouse.scroll(ScrollDirection::Down, 3).await.unwrap();
        mouse.scroll(ScrollDirection::Up, 1).await.unwrap();
    }

    #[tokio::test]
    async fn test_mock_keyboard_type_text() {
        let keyboard = MockKeyboardController::new();

        keyboard.type_text("Hello, ").await.unwrap();
        keyboard.type_text("World!").await.unwrap();

        let typed = keyboard.get_typed_text().await;
        assert_eq!(typed, "Hello, World!");
    }

    #[tokio::test]
    async fn test_mock_keyboard_press_key() {
        let keyboard = MockKeyboardController::new();

        let combo = KeyCombination::char('c').with_ctrl();
        keyboard.press_key(combo).await.unwrap();
    }

    #[tokio::test]
    async fn test_mock_keyboard_key_down_up() {
        let keyboard = MockKeyboardController::new();

        let combo = KeyCombination::key(Key::Shift);
        keyboard.key_down(combo.clone()).await.unwrap();
        keyboard.key_up(combo).await.unwrap();
    }
}
