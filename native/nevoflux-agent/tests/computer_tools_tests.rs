//! Integration tests for computer control tools.
//!
//! These tests verify the computer control tools from Phase 6, including
//! screenshot, mouse, and keyboard tools using the mock computer backend.

use nevoflux_daemon::{
    create_mock_computer, register_computer_tools, GetDisplaysTool, GetMousePositionTool,
    MouseClickTool, MouseDragTool, MouseMoveTool, MouseScrollTool, PressKeyTool, ScreenshotTool,
    ToolExecutor, ToolRegistry, TypeTextTool,
};
use std::sync::Arc;

// ============================================================================
// Mock Computer Creation Tests
// ============================================================================

#[test]
fn test_mock_computer_available() {
    let mock = create_mock_computer();
    assert!(mock.is_available());
}

#[test]
fn test_mock_computer_has_name() {
    let mock = create_mock_computer();
    assert_eq!(mock.name(), "mock");
}

// ============================================================================
// Tool Registration Tests
// ============================================================================

#[test]
fn test_register_computer_tools_with_mock() {
    let mock = Arc::new(create_mock_computer());
    let mut registry = ToolRegistry::empty();
    register_computer_tools(&mut registry, mock);

    // Verify all tools are registered
    assert!(registry.has_tool("computer_screenshot"));
    assert!(registry.has_tool("computer_mouse_move"));
    assert!(registry.has_tool("computer_mouse_click"));
    assert!(registry.has_tool("computer_type_text"));
    assert!(registry.has_tool("computer_get_displays"));
    assert!(registry.has_tool("computer_mouse_position"));
    assert!(registry.has_tool("computer_mouse_scroll"));
    assert!(registry.has_tool("computer_mouse_drag"));
    assert!(registry.has_tool("computer_press_key"));
}

#[test]
fn test_register_computer_tools_count() {
    let mock = Arc::new(create_mock_computer());
    let mut registry = ToolRegistry::empty();
    register_computer_tools(&mut registry, mock);

    // Should have exactly 9 computer tools registered
    let tool_names = registry.tool_names();
    assert_eq!(tool_names.len(), 9);

    // All should be prefixed with "computer_"
    for name in tool_names {
        assert!(
            name.starts_with("computer_"),
            "Tool '{}' should be prefixed with 'computer_'",
            name
        );
    }
}

#[test]
fn test_register_does_not_override_builtin_tools() {
    let mock = Arc::new(create_mock_computer());
    let mut registry = ToolRegistry::new(); // Includes built-in tools

    // Verify built-in tools exist before registration
    assert!(registry.has_tool("read_file"));
    assert!(registry.has_tool("write_file"));
    assert!(registry.has_tool("list_files"));

    register_computer_tools(&mut registry, mock);

    // Built-in tools should still exist
    assert!(registry.has_tool("read_file"));
    assert!(registry.has_tool("write_file"));
    assert!(registry.has_tool("list_files"));

    // Computer tools should also exist
    assert!(registry.has_tool("computer_screenshot"));
    assert!(registry.has_tool("computer_mouse_move"));
}

// ============================================================================
// Tool Execution Tests
// ============================================================================

mod tool_execution {
    use super::*;

    #[tokio::test]
    async fn test_screenshot_tool_executes() {
        let mock = Arc::new(create_mock_computer());
        let tool = ScreenshotTool::new(mock);

        let result = tool
            .execute("computer_screenshot", &serde_json::json!({}))
            .await;
        // Mock should return success with screenshot data
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("width"));
        assert!(content.contains("height"));
        assert!(content.contains("1920")); // Mock returns 1920x1080
        assert!(content.contains("1080"));
    }

    #[tokio::test]
    async fn test_screenshot_tool_with_region() {
        let mock = Arc::new(create_mock_computer());
        let tool = ScreenshotTool::new(mock);

        let result = tool
            .execute(
                "computer_screenshot",
                &serde_json::json!({
                    "region": {
                        "x": 100,
                        "y": 100,
                        "width": 200,
                        "height": 150
                    }
                }),
            )
            .await;
        assert!(result.is_ok());

        let content = result.unwrap();
        // Region screenshot should have the specified dimensions
        assert!(content.contains("200"));
        assert!(content.contains("150"));
    }

    #[tokio::test]
    async fn test_get_displays_tool_executes() {
        let mock = Arc::new(create_mock_computer());
        let tool = GetDisplaysTool::new(mock);

        let result = tool
            .execute("computer_get_displays", &serde_json::json!({}))
            .await;
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("is_primary"));
        assert!(content.contains("1920")); // Mock returns primary 1920x1080
    }

    #[tokio::test]
    async fn test_mouse_move_tool_executes() {
        let mock = Arc::new(create_mock_computer());
        let tool = MouseMoveTool::new(mock);

        let result = tool
            .execute(
                "computer_mouse_move",
                &serde_json::json!({
                    "x": 500,
                    "y": 300
                }),
            )
            .await;
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("500"));
        assert!(content.contains("300"));
    }

    #[tokio::test]
    async fn test_mouse_move_tool_requires_coordinates() {
        let mock = Arc::new(create_mock_computer());
        let tool = MouseMoveTool::new(mock);

        let result = tool
            .execute("computer_mouse_move", &serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mouse_click_tool_executes() {
        let mock = Arc::new(create_mock_computer());
        let tool = MouseClickTool::new(mock);

        // Simple click without coordinates
        let result = tool
            .execute("computer_mouse_click", &serde_json::json!({}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mouse_click_tool_with_options() {
        let mock = Arc::new(create_mock_computer());
        let tool = MouseClickTool::new(mock);

        let result = tool
            .execute(
                "computer_mouse_click",
                &serde_json::json!({
                    "x": 100,
                    "y": 200,
                    "button": "right",
                    "click_type": "double"
                }),
            )
            .await;
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("100"));
        assert!(content.contains("200"));
        assert!(content.contains("Double"));
        assert!(content.contains("Right"));
    }

    #[tokio::test]
    async fn test_mouse_scroll_tool_executes() {
        let mock = Arc::new(create_mock_computer());
        let tool = MouseScrollTool::new(mock);

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

        let content = result.unwrap();
        assert!(content.contains("Down"));
        assert!(content.contains("5"));
    }

    #[tokio::test]
    async fn test_mouse_drag_tool_executes() {
        let mock = Arc::new(create_mock_computer());
        let tool = MouseDragTool::new(mock);

        let result = tool
            .execute(
                "computer_mouse_drag",
                &serde_json::json!({
                    "from_x": 100,
                    "from_y": 100,
                    "to_x": 300,
                    "to_y": 400
                }),
            )
            .await;
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("100"));
        assert!(content.contains("300"));
        assert!(content.contains("400"));
    }

    #[tokio::test]
    async fn test_type_text_tool_executes() {
        let mock = Arc::new(create_mock_computer());
        let tool = TypeTextTool::new(mock);

        let result = tool
            .execute(
                "computer_type_text",
                &serde_json::json!({
                    "text": "Hello, World!"
                }),
            )
            .await;
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("13")); // Length of "Hello, World!"
    }

    #[tokio::test]
    async fn test_type_text_tool_requires_text() {
        let mock = Arc::new(create_mock_computer());
        let tool = TypeTextTool::new(mock);

        let result = tool
            .execute("computer_type_text", &serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_press_key_tool_executes() {
        let mock = Arc::new(create_mock_computer());
        let tool = PressKeyTool::new(mock);

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
        let mock = Arc::new(create_mock_computer());
        let tool = PressKeyTool::new(mock);

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
    async fn test_get_mouse_position_tool_executes() {
        let mock = Arc::new(create_mock_computer());
        let tool = GetMousePositionTool::new(mock);

        let result = tool
            .execute("computer_mouse_position", &serde_json::json!({}))
            .await;
        assert!(result.is_ok());
    }
}

// ============================================================================
// Mock Computer State Tracking Tests
// ============================================================================

mod mock_state_tracking {
    use super::*;
    use nevoflux_computer::{MouseController, ScreenshotProvider};

    #[tokio::test]
    async fn test_mock_tracks_mouse_position() {
        let mock = Arc::new(create_mock_computer());

        // Initial position should be (0, 0)
        let pos = mock.get_position().await.unwrap();
        assert_eq!(pos.x, 0);
        assert_eq!(pos.y, 0);

        // Move mouse
        mock.move_to(nevoflux_computer::Point::new(100, 200))
            .await
            .unwrap();

        // Position should be updated
        let pos = mock.get_position().await.unwrap();
        assert_eq!(pos.x, 100);
        assert_eq!(pos.y, 200);
    }

    #[tokio::test]
    async fn test_mock_tracks_typed_text() {
        let mock = create_mock_computer();

        mock.type_text("Hello ").await.unwrap();
        mock.type_text("World!").await.unwrap();

        let typed = mock.get_typed_text().await;
        assert_eq!(typed, "Hello World!");
    }

    #[tokio::test]
    async fn test_mock_tracks_screenshot_count() {
        let mock = create_mock_computer();

        assert_eq!(mock.get_screenshot_count(), 0);

        mock.capture_screen().await.unwrap();
        assert_eq!(mock.get_screenshot_count(), 1);

        mock.capture_screen().await.unwrap();
        mock.capture_screen().await.unwrap();
        assert_eq!(mock.get_screenshot_count(), 3);
    }
}

// ============================================================================
// Platform Computer Creation Tests
// ============================================================================

mod platform_tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_computer_creation() {
        // On Linux, create_computer returns LinuxComputer which may or may not be available
        // depending on whether X11/Wayland is running
        let computer = nevoflux_daemon::create_computer();
        // Just verify the function exists and can be called
        if let Some(computer) = computer {
            let _ = computer.is_available();
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos_computer_creation() {
        let computer = nevoflux_daemon::create_computer();
        if let Some(computer) = computer {
            let _ = computer.is_available();
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_computer_creation() {
        let computer = nevoflux_daemon::create_computer();
        if let Some(computer) = computer {
            let _ = computer.is_available();
        }
    }
}

// ============================================================================
// Tool Naming Convention Tests
// ============================================================================

#[test]
fn test_all_computer_tools_have_correct_prefix() {
    let mock = Arc::new(create_mock_computer());
    let mut registry = ToolRegistry::empty();
    register_computer_tools(&mut registry, mock);

    let expected_tools = [
        "computer_screenshot",
        "computer_get_displays",
        "computer_mouse_move",
        "computer_mouse_click",
        "computer_mouse_scroll",
        "computer_mouse_drag",
        "computer_mouse_position",
        "computer_type_text",
        "computer_press_key",
    ];

    for tool_name in expected_tools {
        assert!(
            registry.has_tool(tool_name),
            "Missing expected tool: {}",
            tool_name
        );
    }
}

// ============================================================================
// Trait Export Tests
// ============================================================================

use nevoflux_computer::{
    ComputerController, KeyboardController, MouseController, ScreenshotProvider,
};

#[test]
fn test_mock_implements_all_traits() {
    let mock = create_mock_computer();

    // Verify MockComputer implements all required traits
    fn assert_screenshot_provider<T: ScreenshotProvider>(_: &T) {}
    fn assert_mouse_controller<T: MouseController>(_: &T) {}
    fn assert_keyboard_controller<T: KeyboardController>(_: &T) {}
    fn assert_computer_controller<T: ComputerController>(_: &T) {}

    assert_screenshot_provider(&mock);
    assert_mouse_controller(&mock);
    assert_keyboard_controller(&mock);
    assert_computer_controller(&mock);
}

#[tokio::test]
async fn test_mock_computer_displays_has_primary() {
    let mock = create_mock_computer();
    let displays = mock.get_displays().await.unwrap();

    assert_eq!(displays.len(), 1);
    assert!(displays[0].is_primary);
    assert_eq!(displays[0].bounds.width, 1920);
    assert_eq!(displays[0].bounds.height, 1080);
}
