// crates/protocol/src/error.rs

use thiserror::Error;

/// Protocol-level errors
#[derive(Error, Debug)]
pub enum ProtocolError {
    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// MessagePack serialization/deserialization error
    #[error("MessagePack error: {0}")]
    MessagePackError(#[from] rmp_serde::decode::Error),

    /// MessagePack encode error
    #[error("MessagePack encode error: {0}")]
    MessagePackEncodeError(#[from] rmp_serde::encode::Error),

    /// Generic serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Invalid message type
    #[error("Invalid message type: {0}")]
    InvalidMessageType(String),

    /// Missing required field
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid channel
    #[error("Invalid channel: {0}")]
    InvalidChannel(String),
}

/// Result type alias for protocol operations
pub type Result<T> = std::result::Result<T, ProtocolError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_error_display() {
        let err = ProtocolError::SerializationError("test error".into());
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_protocol_error_from_serde_json() {
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let err: ProtocolError = json_err.into();
        assert!(matches!(err, ProtocolError::JsonError(_)));
    }

    #[test]
    fn test_protocol_error_from_rmp() {
        // Use incomplete/truncated MessagePack data to trigger decode error
        let bad_data = vec![0xC4, 0xFF]; // bin8 format with length 255, but no data
        let rmp_err = rmp_serde::from_slice::<i32>(&bad_data).unwrap_err();
        let err: ProtocolError = rmp_err.into();
        assert!(matches!(err, ProtocolError::MessagePackError(_)));
    }
}
