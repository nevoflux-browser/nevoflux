//! Mock backend for testing computer use without real GUI.

use crate::error::Result;
use crate::traits::{ComputerController, KeyboardController, MouseController, ScreenshotProvider};
use crate::types::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicI32, Ordering};
use tokio::sync::RwLock;

/// Mock computer controller for testing.
pub struct MockComputer {
    mouse_x: AtomicI32,
    mouse_y: AtomicI32,
    typed_text: RwLock<String>,
    screenshot_count: AtomicI32,
}

impl MockComputer {
    /// Create a new mock computer.
    pub fn new() -> Self {
        Self {
            mouse_x: AtomicI32::new(0),
            mouse_y: AtomicI32::new(0),
            typed_text: RwLock::new(String::new()),
            screenshot_count: AtomicI32::new(0),
        }
    }

    /// Get the typed text (for testing).
    pub async fn get_typed_text(&self) -> String {
        self.typed_text.read().await.clone()
    }

    /// Get screenshot count (for testing).
    pub fn get_screenshot_count(&self) -> i32 {
        self.screenshot_count.load(Ordering::SeqCst)
    }
}

impl Default for MockComputer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScreenshotProvider for MockComputer {
    async fn capture_screen(&self) -> Result<Screenshot> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        self.screenshot_count.fetch_add(1, Ordering::SeqCst);
        Ok(Screenshot::new(
            1920,
            1080,
            ImageFormat::Png,
            STANDARD.encode("mock_screenshot_data"),
        ))
    }

    async fn capture_display(&self, _display_id: u32) -> Result<Screenshot> {
        self.capture_screen().await
    }

    async fn capture_region(&self, region: Region) -> Result<Screenshot> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        self.screenshot_count.fetch_add(1, Ordering::SeqCst);
        Ok(Screenshot::new(
            region.width,
            region.height,
            ImageFormat::Png,
            STANDARD.encode("mock_region_data"),
        ))
    }

    async fn get_displays(&self) -> Result<Vec<DisplayInfo>> {
        Ok(vec![DisplayInfo::primary(1920, 1080)])
    }
}

#[async_trait]
impl MouseController for MockComputer {
    async fn get_position(&self) -> Result<Point> {
        Ok(Point::new(
            self.mouse_x.load(Ordering::SeqCst),
            self.mouse_y.load(Ordering::SeqCst),
        ))
    }

    async fn move_to(&self, point: Point) -> Result<()> {
        self.mouse_x.store(point.x, Ordering::SeqCst);
        self.mouse_y.store(point.y, Ordering::SeqCst);
        Ok(())
    }

    async fn move_by(&self, dx: i32, dy: i32) -> Result<()> {
        self.mouse_x.fetch_add(dx, Ordering::SeqCst);
        self.mouse_y.fetch_add(dy, Ordering::SeqCst);
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

#[async_trait]
impl KeyboardController for MockComputer {
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

#[async_trait]
impl ComputerController for MockComputer {
    fn name(&self) -> &str {
        "mock"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_computer_screenshot() {
        let mock = MockComputer::new();
        let ss = mock.capture_screen().await.unwrap();
        assert_eq!(ss.width, 1920);
        assert_eq!(mock.get_screenshot_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_computer_mouse() {
        let mock = MockComputer::new();
        mock.move_to(Point::new(100, 200)).await.unwrap();
        let pos = mock.get_position().await.unwrap();
        assert_eq!(pos, Point::new(100, 200));
    }

    #[tokio::test]
    async fn test_mock_computer_keyboard() {
        let mock = MockComputer::new();
        mock.type_text("Hello ").await.unwrap();
        mock.type_text("World!").await.unwrap();
        assert_eq!(mock.get_typed_text().await, "Hello World!");
    }

    #[tokio::test]
    async fn test_mock_computer_is_controller() {
        let mock = MockComputer::new();
        assert_eq!(mock.name(), "mock");
        assert!(mock.is_available());
    }
}
