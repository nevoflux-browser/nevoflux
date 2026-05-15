//! Agent execution module.

pub mod abi;
pub mod auth;
pub mod browser_input;
pub mod code_mode;
pub mod computer_tools;
pub mod roles;
pub mod runner;
pub mod streaming;
pub mod tools;

pub use abi::{
    AgentContent, AgentProcessInput, AgentProcessOutput, AgentResult, HistoryEntry,
    PendingToolCall, ToolResult, ABI_VERSION, ABI_VERSION_FUNC, ALLOC_FUNC, ENTRY_POINT, FREE_FUNC,
    MEMORY_EXPORT,
};
pub use computer_tools::{
    create_mock_computer, register_computer_tools, GetDisplaysTool, GetMousePositionTool,
    MouseClickTool, MouseDragTool, MouseMoveTool, MouseScrollTool, PressKeyTool, ScreenshotTool,
    TypeTextTool,
};
pub use runner::{AgentInput, AgentMode, AgentOutput, AgentRunner, AgentRunnerConfig, ToolCall};
pub use streaming::{
    create_stream_channel, StreamEvent, StreamHandle, StreamSendError, DEFAULT_STREAM_BUFFER_SIZE,
};
pub use tools::{ToolExecutor, ToolRegistry};
