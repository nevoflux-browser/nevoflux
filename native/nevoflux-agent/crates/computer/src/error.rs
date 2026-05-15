//! Error types for the computer crate.

use thiserror::Error;

/// Computer Use error type.
#[derive(Debug, Error)]
pub enum ComputerError {
    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Connection to display server failed.
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Screenshot capture failed.
    #[error("Screenshot failed: {0}")]
    ScreenshotFailed(String),

    /// Mouse operation failed.
    #[error("Mouse operation failed: {0}")]
    MouseFailed(String),

    /// Keyboard operation failed.
    #[error("Keyboard operation failed: {0}")]
    KeyboardFailed(String),

    /// General input operation failed.
    #[error("Input operation failed: {0}")]
    InputFailed(String),

    /// Invalid coordinates.
    #[error("Invalid coordinates: ({0}, {1})")]
    InvalidCoordinates(i32, i32),

    /// Invalid key.
    #[error("Invalid key: {0}")]
    InvalidKey(String),

    /// Image encoding error.
    #[error("Image encoding error: {0}")]
    ImageEncoding(String),

    /// Permission denied.
    #[error("Permission denied for computer use")]
    PermissionDenied,

    /// Not supported on this platform.
    #[error("Not supported on this platform: {0}")]
    NotSupported(String),

    /// Timeout.
    #[error("Operation timed out")]
    Timeout,
}

/// Result type for computer operations.
pub type Result<T> = std::result::Result<T, ComputerError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ComputerError::ScreenshotFailed("no display".into());
        assert_eq!(err.to_string(), "Screenshot failed: no display");
    }

    #[test]
    fn test_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: ComputerError = io_err.into();
        assert!(err.to_string().contains("IO error"));
    }

    #[test]
    fn test_error_invalid_coordinates() {
        let err = ComputerError::InvalidCoordinates(-100, -200);
        assert!(err.to_string().contains("-100"));
        assert!(err.to_string().contains("-200"));
    }

    #[test]
    fn test_error_invalid_key() {
        let err = ComputerError::InvalidKey("BadKey".into());
        assert!(err.to_string().contains("BadKey"));
    }

    #[test]
    fn test_error_permission_denied() {
        let err = ComputerError::PermissionDenied;
        assert!(err.to_string().contains("Permission denied"));
    }

    #[test]
    fn test_error_not_supported() {
        let err = ComputerError::NotSupported("mouse simulation".into());
        assert!(err.to_string().contains("mouse simulation"));
    }

    #[test]
    fn test_error_timeout() {
        let err = ComputerError::Timeout;
        assert_eq!(err.to_string(), "Operation timed out");
    }
}
