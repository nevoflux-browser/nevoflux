//! Computer control tools for the agent.
//!
//! This module provides tools for controlling the computer through screenshots,
//! mouse movements, clicks, and keyboard input. These tools wrap the platform-specific
//! implementations from the `nevoflux-computer` crate.

use crate::agent::tools::ToolExecutor;
use crate::error::{DaemonError, Result};
use async_trait::async_trait;
use nevoflux_computer::{
    ClickType, ComputerController, Key, KeyCombination, KeyOrChar, KeyboardController, MouseButton,
    MouseController, Point, Region, ScreenshotProvider, ScrollDirection,
};
use std::sync::Arc;

// ============================================================================
// Screenshot Tool
// ============================================================================

/// Tool for taking screenshots.
///
/// Captures the entire screen or a specific region.
pub struct ScreenshotTool<P: ScreenshotProvider + Send + Sync> {
    provider: Arc<P>,
}

impl<P: ScreenshotProvider + Send + Sync> ScreenshotTool<P> {
    /// Create a new screenshot tool with the given provider.
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: ScreenshotProvider + Send + Sync + 'static> ToolExecutor for ScreenshotTool<P> {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        // Check for optional region parameter
        let screenshot =
            if let Some(region) = arguments.get("region") {
                let x = region.get("x").and_then(|v| v.as_i64()).ok_or_else(|| {
                    DaemonError::InternalError("Missing 'x' in region".to_string())
                })? as i32;
                let y = region.get("y").and_then(|v| v.as_i64()).ok_or_else(|| {
                    DaemonError::InternalError("Missing 'y' in region".to_string())
                })? as i32;
                let width = region
                    .get("width")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| {
                        DaemonError::InternalError("Missing 'width' in region".to_string())
                    })? as u32;
                let height = region
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| {
                        DaemonError::InternalError("Missing 'height' in region".to_string())
                    })? as u32;

                let region = Region::new(x, y, width, height);
                self.provider
                    .capture_region(region)
                    .await
                    .map_err(|e| DaemonError::InternalError(format!("Screenshot failed: {}", e)))?
            } else {
                self.provider
                    .capture_screen()
                    .await
                    .map_err(|e| DaemonError::InternalError(format!("Screenshot failed: {}", e)))?
            };

        serde_json::to_string(&screenshot).map_err(|e| {
            DaemonError::SerializationError(format!("Failed to serialize screenshot: {}", e))
        })
    }
}

// ============================================================================
// Mouse Move Tool
// ============================================================================

/// Tool for moving the mouse to a specific position.
pub struct MouseMoveTool<M: MouseController + Send + Sync> {
    controller: Arc<M>,
}

impl<M: MouseController + Send + Sync> MouseMoveTool<M> {
    /// Create a new mouse move tool with the given controller.
    pub fn new(controller: Arc<M>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl<M: MouseController + Send + Sync + 'static> ToolExecutor for MouseMoveTool<M> {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        let x = arguments
            .get("x")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DaemonError::InternalError("Missing 'x' argument".to_string()))?
            as i32;
        let y = arguments
            .get("y")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DaemonError::InternalError("Missing 'y' argument".to_string()))?
            as i32;

        let point = Point::new(x, y);
        self.controller
            .move_to(point)
            .await
            .map_err(|e| DaemonError::InternalError(format!("Mouse move failed: {}", e)))?;

        Ok(format!("Moved mouse to ({}, {})", x, y))
    }
}

// ============================================================================
// Mouse Click Tool
// ============================================================================

/// Tool for clicking the mouse.
pub struct MouseClickTool<M: MouseController + Send + Sync> {
    controller: Arc<M>,
}

impl<M: MouseController + Send + Sync> MouseClickTool<M> {
    /// Create a new mouse click tool with the given controller.
    pub fn new(controller: Arc<M>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl<M: MouseController + Send + Sync + 'static> ToolExecutor for MouseClickTool<M> {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        // Parse button (default: left)
        let button = arguments
            .get("button")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "right" => MouseButton::Right,
                "middle" => MouseButton::Middle,
                _ => MouseButton::Left,
            })
            .unwrap_or(MouseButton::Left);

        // Parse click type (default: single)
        let click_type = arguments
            .get("click_type")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "double" => ClickType::Double,
                "triple" => ClickType::Triple,
                _ => ClickType::Single,
            })
            .unwrap_or(ClickType::Single);

        // Optional position - if provided, move to it first
        if let (Some(x), Some(y)) = (
            arguments.get("x").and_then(|v| v.as_i64()),
            arguments.get("y").and_then(|v| v.as_i64()),
        ) {
            let point = Point::new(x as i32, y as i32);
            self.controller
                .click_at(point, button, click_type)
                .await
                .map_err(|e| DaemonError::InternalError(format!("Mouse click failed: {}", e)))?;
            Ok(format!(
                "Clicked {:?} {:?} at ({}, {})",
                click_type, button, x, y
            ))
        } else {
            self.controller
                .click(button, click_type)
                .await
                .map_err(|e| DaemonError::InternalError(format!("Mouse click failed: {}", e)))?;
            Ok(format!("Clicked {:?} {:?}", click_type, button))
        }
    }
}

// ============================================================================
// Mouse Scroll Tool
// ============================================================================

/// Tool for scrolling the mouse wheel.
pub struct MouseScrollTool<M: MouseController + Send + Sync> {
    controller: Arc<M>,
}

impl<M: MouseController + Send + Sync> MouseScrollTool<M> {
    /// Create a new mouse scroll tool with the given controller.
    pub fn new(controller: Arc<M>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl<M: MouseController + Send + Sync + 'static> ToolExecutor for MouseScrollTool<M> {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        let direction = arguments
            .get("direction")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "down" => ScrollDirection::Down,
                "left" => ScrollDirection::Left,
                "right" => ScrollDirection::Right,
                _ => ScrollDirection::Up,
            })
            .unwrap_or(ScrollDirection::Down);

        let amount = arguments
            .get("amount")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32;

        self.controller
            .scroll(direction, amount)
            .await
            .map_err(|e| DaemonError::InternalError(format!("Mouse scroll failed: {}", e)))?;

        Ok(format!("Scrolled {:?} by {}", direction, amount))
    }
}

// ============================================================================
// Mouse Drag Tool
// ============================================================================

/// Tool for dragging the mouse.
pub struct MouseDragTool<M: MouseController + Send + Sync> {
    controller: Arc<M>,
}

impl<M: MouseController + Send + Sync> MouseDragTool<M> {
    /// Create a new mouse drag tool with the given controller.
    pub fn new(controller: Arc<M>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl<M: MouseController + Send + Sync + 'static> ToolExecutor for MouseDragTool<M> {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        let from_x = arguments
            .get("from_x")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DaemonError::InternalError("Missing 'from_x' argument".to_string()))?
            as i32;
        let from_y = arguments
            .get("from_y")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DaemonError::InternalError("Missing 'from_y' argument".to_string()))?
            as i32;
        let to_x = arguments
            .get("to_x")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DaemonError::InternalError("Missing 'to_x' argument".to_string()))?
            as i32;
        let to_y = arguments
            .get("to_y")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DaemonError::InternalError("Missing 'to_y' argument".to_string()))?
            as i32;

        let button = arguments
            .get("button")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "right" => MouseButton::Right,
                "middle" => MouseButton::Middle,
                _ => MouseButton::Left,
            })
            .unwrap_or(MouseButton::Left);

        let from = Point::new(from_x, from_y);
        let to = Point::new(to_x, to_y);

        self.controller
            .drag(from, to, button)
            .await
            .map_err(|e| DaemonError::InternalError(format!("Mouse drag failed: {}", e)))?;

        Ok(format!(
            "Dragged from ({}, {}) to ({}, {})",
            from_x, from_y, to_x, to_y
        ))
    }
}

// ============================================================================
// Type Text Tool
// ============================================================================

/// Tool for typing text.
pub struct TypeTextTool<K: KeyboardController + Send + Sync> {
    controller: Arc<K>,
}

impl<K: KeyboardController + Send + Sync> TypeTextTool<K> {
    /// Create a new type text tool with the given controller.
    pub fn new(controller: Arc<K>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl<K: KeyboardController + Send + Sync + 'static> ToolExecutor for TypeTextTool<K> {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        let text = arguments
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DaemonError::InternalError("Missing 'text' argument".to_string()))?;

        self.controller
            .type_text(text)
            .await
            .map_err(|e| DaemonError::InternalError(format!("Type text failed: {}", e)))?;

        Ok(format!("Typed {} characters", text.len()))
    }
}

// ============================================================================
// Press Key Tool
// ============================================================================

/// Tool for pressing key combinations.
pub struct PressKeyTool<K: KeyboardController + Send + Sync> {
    controller: Arc<K>,
}

impl<K: KeyboardController + Send + Sync> PressKeyTool<K> {
    /// Create a new press key tool with the given controller.
    pub fn new(controller: Arc<K>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl<K: KeyboardController + Send + Sync + 'static> ToolExecutor for PressKeyTool<K> {
    async fn execute(&self, _name: &str, arguments: &serde_json::Value) -> Result<String> {
        // Parse the key or character
        let key_or_char = if let Some(key_str) = arguments.get("key").and_then(|v| v.as_str()) {
            parse_key(key_str)?
        } else if let Some(char_str) = arguments.get("char").and_then(|v| v.as_str()) {
            if char_str.len() != 1 {
                return Err(DaemonError::InternalError(
                    "char must be a single character".to_string(),
                ));
            }
            KeyOrChar::Char(char_str.chars().next().unwrap())
        } else {
            return Err(DaemonError::InternalError(
                "Missing 'key' or 'char' argument".to_string(),
            ));
        };

        // Build the key combination
        let mut combination = KeyCombination {
            key: key_or_char,
            modifiers: Vec::new(),
        };

        // Parse optional modifiers
        if let Some(modifiers) = arguments.get("modifiers").and_then(|v| v.as_array()) {
            for modifier in modifiers {
                if let Some(mod_str) = modifier.as_str() {
                    match mod_str.to_lowercase().as_str() {
                        "shift" => combination = combination.with_shift(),
                        "ctrl" | "control" => combination = combination.with_ctrl(),
                        "alt" => combination = combination.with_alt(),
                        "meta" | "cmd" | "command" | "win" | "windows" => {
                            combination = combination.with_meta()
                        }
                        _ => {}
                    }
                }
            }
        }

        self.controller
            .press_key(combination.clone())
            .await
            .map_err(|e| DaemonError::InternalError(format!("Press key failed: {}", e)))?;

        Ok(format!("Pressed key combination: {:?}", combination))
    }
}

/// Parse a key string into a Key enum.
fn parse_key(key_str: &str) -> Result<KeyOrChar> {
    let key = match key_str.to_lowercase().as_str() {
        // Modifiers
        "shift" => Key::Shift,
        "ctrl" | "control" => Key::Control,
        "alt" => Key::Alt,
        "meta" | "cmd" | "command" | "win" | "windows" => Key::Meta,

        // Function keys
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,

        // Navigation
        "escape" | "esc" => Key::Escape,
        "tab" => Key::Tab,
        "capslock" | "caps_lock" => Key::CapsLock,
        "space" => Key::Space,
        "enter" | "return" => Key::Enter,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "insert" | "ins" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "page_up" => Key::PageUp,
        "pagedown" | "page_down" => Key::PageDown,
        "up" | "arrowup" | "arrow_up" => Key::ArrowUp,
        "down" | "arrowdown" | "arrow_down" => Key::ArrowDown,
        "left" | "arrowleft" | "arrow_left" => Key::ArrowLeft,
        "right" | "arrowright" | "arrow_right" => Key::ArrowRight,

        // Other
        "printscreen" | "print_screen" => Key::PrintScreen,
        "scrolllock" | "scroll_lock" => Key::ScrollLock,
        "pause" => Key::Pause,
        "numlock" | "num_lock" => Key::NumLock,

        // If single character, treat as character key
        s if s.len() == 1 => {
            return Ok(KeyOrChar::Char(s.chars().next().unwrap()));
        }

        _ => {
            return Err(DaemonError::InternalError(format!(
                "Unknown key: {}",
                key_str
            )));
        }
    };

    Ok(KeyOrChar::Key(key))
}

// ============================================================================
// Get Display Info Tool
// ============================================================================

/// Tool for getting display information.
pub struct GetDisplaysTool<P: ScreenshotProvider + Send + Sync> {
    provider: Arc<P>,
}

impl<P: ScreenshotProvider + Send + Sync> GetDisplaysTool<P> {
    /// Create a new get displays tool with the given provider.
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: ScreenshotProvider + Send + Sync + 'static> ToolExecutor for GetDisplaysTool<P> {
    async fn execute(&self, _name: &str, _arguments: &serde_json::Value) -> Result<String> {
        let displays = self
            .provider
            .get_displays()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Get displays failed: {}", e)))?;

        serde_json::to_string(&displays).map_err(|e| {
            DaemonError::SerializationError(format!("Failed to serialize displays: {}", e))
        })
    }
}

// ============================================================================
// Get Mouse Position Tool
// ============================================================================

/// Tool for getting the current mouse position.
pub struct GetMousePositionTool<M: MouseController + Send + Sync> {
    controller: Arc<M>,
}

impl<M: MouseController + Send + Sync> GetMousePositionTool<M> {
    /// Create a new get mouse position tool with the given controller.
    pub fn new(controller: Arc<M>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl<M: MouseController + Send + Sync + 'static> ToolExecutor for GetMousePositionTool<M> {
    async fn execute(&self, _name: &str, _arguments: &serde_json::Value) -> Result<String> {
        let position =
            self.controller.get_position().await.map_err(|e| {
                DaemonError::InternalError(format!("Get mouse position failed: {}", e))
            })?;

        serde_json::to_string(&position).map_err(|e| {
            DaemonError::SerializationError(format!("Failed to serialize position: {}", e))
        })
    }
}

// ============================================================================
// Platform-specific Computer Creation
// ============================================================================

/// Create a platform-specific computer controller.
///
/// Returns `None` if the controller cannot be created on the current platform.
#[cfg(target_os = "linux")]
pub fn create_computer() -> Option<nevoflux_computer::LinuxComputer> {
    nevoflux_computer::LinuxComputer::new().ok()
}

/// Create a platform-specific computer controller.
///
/// Returns `None` if the controller cannot be created on the current platform.
#[cfg(target_os = "macos")]
pub fn create_computer() -> Option<nevoflux_computer::MacOsComputer> {
    nevoflux_computer::MacOsComputer::new().ok()
}

/// Create a platform-specific computer controller.
///
/// Returns `None` if the controller cannot be created on the current platform.
#[cfg(target_os = "windows")]
pub fn create_computer() -> Option<nevoflux_computer::WindowsComputer> {
    nevoflux_computer::WindowsComputer::new().ok()
}

/// Create a mock computer controller for testing.
pub fn create_mock_computer() -> nevoflux_computer::MockComputer {
    nevoflux_computer::MockComputer::new()
}

// ============================================================================
// Tool Registration Helper
// ============================================================================

use crate::agent::tools::ToolRegistry;

/// Register computer control tools with the given controller.
///
/// This registers all computer control tools with the provided controller,
/// which must implement `ScreenshotProvider`, `MouseController`, and `KeyboardController`.
pub fn register_computer_tools<C>(registry: &mut ToolRegistry, controller: Arc<C>)
where
    C: ComputerController + 'static,
{
    // Screenshot tools
    registry.register(
        "computer_screenshot",
        Box::new(ScreenshotTool::new(controller.clone())),
    );
    registry.register(
        "computer_get_displays",
        Box::new(GetDisplaysTool::new(controller.clone())),
    );

    // Mouse tools
    registry.register(
        "computer_mouse_move",
        Box::new(MouseMoveTool::new(controller.clone())),
    );
    registry.register(
        "computer_mouse_click",
        Box::new(MouseClickTool::new(controller.clone())),
    );
    registry.register(
        "computer_mouse_scroll",
        Box::new(MouseScrollTool::new(controller.clone())),
    );
    registry.register(
        "computer_mouse_drag",
        Box::new(MouseDragTool::new(controller.clone())),
    );
    registry.register(
        "computer_mouse_position",
        Box::new(GetMousePositionTool::new(controller.clone())),
    );

    // Keyboard tools
    registry.register(
        "computer_type_text",
        Box::new(TypeTextTool::new(controller.clone())),
    );
    registry.register(
        "computer_press_key",
        Box::new(PressKeyTool::new(controller)),
    );
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_computer::MockComputer;

    fn create_test_controller() -> Arc<MockComputer> {
        Arc::new(MockComputer::new())
    }

    #[tokio::test]
    async fn test_screenshot_tool() {
        let controller = create_test_controller();
        let tool = ScreenshotTool::new(controller);

        let result = tool
            .execute("computer_screenshot", &serde_json::json!({}))
            .await;
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("width"));
        assert!(content.contains("height"));
    }

    #[tokio::test]
    async fn test_screenshot_tool_with_region() {
        let controller = create_test_controller();
        let tool = ScreenshotTool::new(controller);

        let result = tool
            .execute(
                "computer_screenshot",
                &serde_json::json!({
                    "region": {
                        "x": 0,
                        "y": 0,
                        "width": 100,
                        "height": 100
                    }
                }),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mouse_move_tool() {
        let controller = create_test_controller();
        let tool = MouseMoveTool::new(controller);

        let result = tool
            .execute(
                "computer_mouse_move",
                &serde_json::json!({
                    "x": 100,
                    "y": 200
                }),
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("100"));
    }

    #[tokio::test]
    async fn test_mouse_move_tool_missing_args() {
        let controller = create_test_controller();
        let tool = MouseMoveTool::new(controller);

        let result = tool
            .execute("computer_mouse_move", &serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mouse_click_tool() {
        let controller = create_test_controller();
        let tool = MouseClickTool::new(controller);

        let result = tool
            .execute("computer_mouse_click", &serde_json::json!({}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mouse_click_tool_with_position() {
        let controller = create_test_controller();
        let tool = MouseClickTool::new(controller);

        let result = tool
            .execute(
                "computer_mouse_click",
                &serde_json::json!({
                    "x": 500,
                    "y": 300,
                    "button": "right",
                    "click_type": "double"
                }),
            )
            .await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("500"));
        assert!(content.contains("300"));
    }

    #[tokio::test]
    async fn test_mouse_scroll_tool() {
        let controller = create_test_controller();
        let tool = MouseScrollTool::new(controller);

        let result = tool
            .execute(
                "computer_mouse_scroll",
                &serde_json::json!({
                    "direction": "down",
                    "amount": 5
                }),
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("5"));
    }

    #[tokio::test]
    async fn test_mouse_drag_tool() {
        let controller = create_test_controller();
        let tool = MouseDragTool::new(controller);

        let result = tool
            .execute(
                "computer_mouse_drag",
                &serde_json::json!({
                    "from_x": 100,
                    "from_y": 100,
                    "to_x": 200,
                    "to_y": 200
                }),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_type_text_tool() {
        let controller = create_test_controller();
        let tool = TypeTextTool::new(controller);

        let result = tool
            .execute(
                "computer_type_text",
                &serde_json::json!({
                    "text": "Hello, World!"
                }),
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("13")); // Length of "Hello, World!"
    }

    #[tokio::test]
    async fn test_press_key_tool() {
        let controller = create_test_controller();
        let tool = PressKeyTool::new(controller);

        let result = tool
            .execute(
                "computer_press_key",
                &serde_json::json!({
                    "key": "enter"
                }),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_press_key_tool_with_modifiers() {
        let controller = create_test_controller();
        let tool = PressKeyTool::new(controller);

        let result = tool
            .execute(
                "computer_press_key",
                &serde_json::json!({
                    "char": "c",
                    "modifiers": ["ctrl"]
                }),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_displays_tool() {
        let controller = create_test_controller();
        let tool = GetDisplaysTool::new(controller);

        let result = tool
            .execute("computer_get_displays", &serde_json::json!({}))
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("is_primary"));
    }

    #[tokio::test]
    async fn test_get_mouse_position_tool() {
        let controller = create_test_controller();
        let tool = GetMousePositionTool::new(controller);

        let result = tool
            .execute("computer_mouse_position", &serde_json::json!({}))
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_key_special_keys() {
        assert!(matches!(parse_key("enter"), Ok(KeyOrChar::Key(Key::Enter))));
        assert!(matches!(
            parse_key("escape"),
            Ok(KeyOrChar::Key(Key::Escape))
        ));
        assert!(matches!(parse_key("tab"), Ok(KeyOrChar::Key(Key::Tab))));
        assert!(matches!(parse_key("f1"), Ok(KeyOrChar::Key(Key::F1))));
        assert!(matches!(
            parse_key("ctrl"),
            Ok(KeyOrChar::Key(Key::Control))
        ));
    }

    #[test]
    fn test_parse_key_single_char() {
        assert!(matches!(parse_key("a"), Ok(KeyOrChar::Char('a'))));
        assert!(matches!(parse_key("1"), Ok(KeyOrChar::Char('1'))));
    }

    #[test]
    fn test_parse_key_unknown() {
        assert!(parse_key("unknown_key").is_err());
    }

    #[test]
    fn test_register_computer_tools() {
        let controller = create_test_controller();
        let mut registry = ToolRegistry::empty();

        register_computer_tools(&mut registry, controller);

        assert!(registry.has_tool("computer_screenshot"));
        assert!(registry.has_tool("computer_get_displays"));
        assert!(registry.has_tool("computer_mouse_move"));
        assert!(registry.has_tool("computer_mouse_click"));
        assert!(registry.has_tool("computer_mouse_scroll"));
        assert!(registry.has_tool("computer_mouse_drag"));
        assert!(registry.has_tool("computer_mouse_position"));
        assert!(registry.has_tool("computer_type_text"));
        assert!(registry.has_tool("computer_press_key"));
    }

    #[test]
    fn test_create_mock_computer() {
        let computer = create_mock_computer();
        assert!(computer.is_available());
    }
}
