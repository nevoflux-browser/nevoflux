//! Error types for the daemon.

use crate::retry::Retryable;
use thiserror::Error;

/// Result type for daemon operations.
pub type Result<T> = std::result::Result<T, DaemonError>;

/// Errors that can occur during daemon operations.
#[derive(Debug, Error)]
pub enum DaemonError {
    /// Storage error.
    #[error("Storage error: {0}")]
    StorageError(#[from] nevoflux_storage::StorageError),

    /// Session not found.
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    /// Proxy not found.
    #[error("Proxy not found: {0}")]
    ProxyNotFound(String),

    /// Request not found.
    #[error("Request not found: {0}")]
    RequestNotFound(String),

    /// Session already has an active request.
    #[error("Session {0} already has an active request")]
    SessionBusy(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Router error.
    #[error("Router error: {0}")]
    RouterError(String),

    /// Context building error.
    #[error("Context error: {0}")]
    ContextError(String),

    /// Permission denied.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Internal error.
    #[error("Internal error: {0}")]
    InternalError(String),

    /// No available port in range.
    #[error("No available port in range")]
    PortExhausted,

    /// Connection failed (transient).
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Operation timed out (transient).
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Channel closed unexpectedly (transient).
    #[error("Channel closed: {0}")]
    ChannelClosed(String),

    /// Invalid request (permanent).
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Skill asset not found.
    #[error("skill asset not found: skill={skill}, path={path}")]
    SkillAssetNotFound { skill: String, path: String },

    /// Invalid composition.
    #[error("invalid composition: {reason}")]
    InvalidComposition { reason: String },

    /// Lint timeout for composition.
    #[error("lint timeout for composition {composition_id}")]
    LintTimeout { composition_id: String },

    /// Template substitution failed.
    #[error("template substitution failed for '{template}': missing placeholders {missing:?}")]
    TemplateSubstitutionFailed {
        template: String,
        missing: Vec<String>,
    },
}

impl Retryable for DaemonError {
    /// Returns true if this error is transient and the operation should be retried.
    ///
    /// Transient errors (retryable):
    /// - `ConnectionFailed` - Network connection issues may resolve
    /// - `Timeout` - Temporary overload or network latency
    /// - `ChannelClosed` - Communication channel can be re-established
    ///
    /// Permanent errors (not retryable):
    /// - `SessionNotFound` - Resource doesn't exist
    /// - `ProxyNotFound` - Resource doesn't exist
    /// - `RequestNotFound` - Resource doesn't exist
    /// - `SessionBusy` - State conflict
    /// - `ConfigError` - Configuration issues require user intervention
    /// - `RouterError` - Routing logic errors
    /// - `ContextError` - Context building failures
    /// - `PermissionDenied` - Authorization failures
    /// - `SerializationError` - Data format issues
    /// - `InternalError` - Implementation bugs
    /// - `PortExhausted` - No ports available
    /// - `InvalidRequest` - Client-side errors
    /// - `StorageError` - Depends on underlying error (treated as non-retryable)
    /// - `IoError` - Depends on kind (treated as non-retryable by default)
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            DaemonError::ConnectionFailed(_)
                | DaemonError::Timeout(_)
                | DaemonError::ChannelClosed(_)
        )
    }
}

impl From<serde_json::Error> for DaemonError {
    fn from(err: serde_json::Error) -> Self {
        DaemonError::SerializationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = DaemonError::SessionNotFound("sess-001".to_string());
        assert!(err.to_string().contains("sess-001"));
    }

    #[test]
    fn test_error_from_serde() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let daemon_err: DaemonError = json_err.into();
        assert!(matches!(daemon_err, DaemonError::SerializationError(_)));
    }

    #[test]
    fn test_connection_failed_is_retryable() {
        let err = DaemonError::ConnectionFailed("connection refused".to_string());
        assert!(err.is_retryable());
        assert!(err.to_string().contains("connection refused"));
    }

    #[test]
    fn test_timeout_is_retryable() {
        let err = DaemonError::Timeout("operation timed out".to_string());
        assert!(err.is_retryable());
        assert!(err.to_string().contains("operation timed out"));
    }

    #[test]
    fn test_channel_closed_is_retryable() {
        let err = DaemonError::ChannelClosed("receiver dropped".to_string());
        assert!(err.is_retryable());
        assert!(err.to_string().contains("receiver dropped"));
    }

    #[test]
    fn test_session_not_found_is_not_retryable() {
        let err = DaemonError::SessionNotFound("sess-001".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_permission_denied_is_not_retryable() {
        let err = DaemonError::PermissionDenied("access denied".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_invalid_request_is_not_retryable() {
        let err = DaemonError::InvalidRequest("malformed JSON".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_internal_error_is_not_retryable() {
        let err = DaemonError::InternalError("unexpected state".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_config_error_is_not_retryable() {
        let err = DaemonError::ConfigError("invalid config".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_serialization_error_is_not_retryable() {
        let err = DaemonError::SerializationError("parse error".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_port_exhausted_is_not_retryable() {
        let err = DaemonError::PortExhausted;
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_router_error_is_not_retryable() {
        let err = DaemonError::RouterError("routing failed".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_context_error_is_not_retryable() {
        let err = DaemonError::ContextError("context build failed".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_proxy_not_found_is_not_retryable() {
        let err = DaemonError::ProxyNotFound("proxy-001".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_request_not_found_is_not_retryable() {
        let err = DaemonError::RequestNotFound("req-001".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_session_busy_is_not_retryable() {
        let err = DaemonError::SessionBusy("sess-001".to_string());
        assert!(!err.is_retryable());
    }
}
