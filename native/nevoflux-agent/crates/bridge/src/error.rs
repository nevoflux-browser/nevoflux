//! Error types for the bridge crate.

use thiserror::Error;

/// Bridge error type.
#[derive(Debug, Error)]
pub enum BridgeError {
    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Daemon not running.
    #[error("Daemon not running")]
    DaemonNotRunning,

    /// Daemon connection failed.
    #[error("Failed to connect to daemon: {0}")]
    ConnectionFailed(String),

    /// Port file not found.
    #[error("Port file not found: {0}")]
    PortFileNotFound(String),

    /// Invalid port file contents.
    #[error("Invalid port file: {0}")]
    InvalidPortFile(String),

    /// Native messaging protocol error.
    #[error("Native messaging error: {0}")]
    NativeMessaging(String),

    /// Channel closed.
    #[error("Channel closed")]
    ChannelClosed,

    /// Timeout.
    #[error("Operation timed out")]
    Timeout,

    /// Failed to launch daemon.
    #[error("Failed to launch daemon: {0}")]
    DaemonLaunchFailed(String),

    /// Disconnected from daemon.
    #[error("Disconnected from daemon")]
    Disconnected,

    /// Reconnection failed after max retries.
    #[error("Reconnection failed after {0} attempts")]
    ReconnectionFailed(u32),
}

/// Result type for bridge operations.
pub type Result<T> = std::result::Result<T, BridgeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = BridgeError::DaemonNotRunning;
        assert_eq!(err.to_string(), "Daemon not running");
    }

    #[test]
    fn test_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: BridgeError = io_err.into();
        assert!(err.to_string().contains("IO error"));
    }

    #[test]
    fn test_error_json() {
        let json_err = serde_json::from_str::<String>("invalid").unwrap_err();
        let err: BridgeError = json_err.into();
        assert!(err.to_string().contains("JSON error"));
    }

    #[test]
    fn test_error_port_file_not_found() {
        let err = BridgeError::PortFileNotFound("/path/to/file".into());
        assert!(err.to_string().contains("/path/to/file"));
    }

    #[test]
    fn test_error_connection_failed() {
        let err = BridgeError::ConnectionFailed("connection refused".into());
        assert!(err.to_string().contains("connection refused"));
    }

    #[test]
    fn test_error_native_messaging() {
        let err = BridgeError::NativeMessaging("invalid length".into());
        assert!(err.to_string().contains("invalid length"));
    }

    #[test]
    fn test_error_timeout() {
        let err = BridgeError::Timeout;
        assert_eq!(err.to_string(), "Operation timed out");
    }

    #[test]
    fn test_error_daemon_launch_failed() {
        let err = BridgeError::DaemonLaunchFailed("not found".into());
        assert!(err.to_string().contains("not found"));
    }
}
