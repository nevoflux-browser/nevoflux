//! Computer Use crate for NevoFlux Agent.
//!
//! Provides screenshot, mouse, and keyboard control functionality.
//!
//! # Architecture
//!
//! This crate defines traits for computer control operations that can be
//! implemented by platform-specific backends:
//!
//! - [`ScreenshotProvider`] - Screen capture operations
//! - [`MouseController`] - Mouse movement and clicks
//! - [`KeyboardController`] - Text typing and key presses
//! - [`ComputerController`] - Combined trait for full control
//!
//! # Example
//!
//! ```ignore
//! use nevoflux_computer::{ComputerController, Point, MouseButton, ClickType};
//!
//! async fn example(controller: &impl ComputerController) -> Result<(), Box<dyn std::error::Error>> {
//!     // Take a screenshot
//!     let screenshot = controller.capture_screen().await?;
//!     println!("Captured {}x{} screenshot", screenshot.width, screenshot.height);
//!
//!     // Move mouse and click
//!     controller.move_to(Point::new(100, 200)).await?;
//!     controller.click(MouseButton::Left, ClickType::Single).await?;
//!
//!     // Type some text
//!     controller.type_text("Hello, world!").await?;
//!
//!     Ok(())
//! }
//! ```

pub mod error;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod mock;
pub mod traits;
pub mod types;
pub mod windows;

pub use error::{ComputerError, Result};
#[cfg(target_os = "linux")]
pub use linux::LinuxComputer;
#[cfg(target_os = "macos")]
pub use macos::MacOsComputer;
pub use mock::MockComputer;
pub use traits::{ComputerController, KeyboardController, MouseController, ScreenshotProvider};
pub use types::{
    ClickType, DisplayInfo, ImageFormat, Key, KeyCombination, KeyOrChar, MouseButton, Point,
    Region, Screenshot, ScrollDirection,
};
pub use windows::WindowsComputer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exports_available() {
        // Verify types are exported
        let _ = Point::new(0, 0);
        let _ = Region::new(0, 0, 100, 100);
        let _ = MouseButton::Left;
        let _ = ClickType::Single;
        let _ = ScrollDirection::Up;
        let _ = Key::Enter;
        let _ = KeyCombination::key(Key::Enter);
        let _ = ImageFormat::Png;
    }

    #[test]
    fn test_error_types() {
        let err = ComputerError::ScreenshotFailed("test".into());
        assert!(err.to_string().contains("Screenshot failed"));
    }

    #[test]
    fn test_point_operations() {
        let p1 = Point::new(0, 0);
        let p2 = Point::new(3, 4);
        assert!((p1.distance_to(&p2) - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_region_operations() {
        let r = Region::new(10, 10, 100, 100);
        assert!(r.contains(&Point::new(50, 50)));
        assert!(!r.contains(&Point::new(5, 5)));
    }

    #[test]
    fn test_key_combination() {
        let combo = KeyCombination::char('c').with_ctrl();
        assert!(combo.modifiers.contains(&Key::Control));
    }

    #[test]
    fn test_screenshot_creation() {
        let ss = Screenshot::new(1920, 1080, ImageFormat::Png, "data".into());
        assert_eq!(ss.width, 1920);
        assert_eq!(ss.height, 1080);
    }

    #[test]
    fn test_display_info() {
        let display = DisplayInfo::primary(1920, 1080);
        assert!(display.is_primary);
        assert_eq!(display.bounds.width, 1920);
    }
}
