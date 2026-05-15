//! Native Messaging protocol implementation.
//!
//! Native Messaging uses stdin/stdout with 4-byte little-endian length prefix.

use crate::error::{BridgeError, Result};
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Maximum message size (1 MB).
pub const MAX_MESSAGE_SIZE: u32 = 1024 * 1024;

/// Read a message from the native messaging input.
///
/// Message format: 4-byte little-endian length + JSON payload
pub async fn read_message<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncReadExt + Unpin,
    T: DeserializeOwned,
{
    // Read 4-byte length prefix
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf);

    // Validate length
    if len > MAX_MESSAGE_SIZE {
        return Err(BridgeError::NativeMessaging(format!(
            "Message too large: {} bytes (max {})",
            len, MAX_MESSAGE_SIZE
        )));
    }

    if len == 0 {
        return Err(BridgeError::NativeMessaging("Empty message".into()));
    }

    // Read message payload
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf).await?;

    // Deserialize JSON
    serde_json::from_slice(&buf).map_err(BridgeError::from)
}

/// Write a message to the native messaging output.
///
/// Message format: 4-byte little-endian length + JSON payload
pub async fn write_message<W, T>(writer: &mut W, message: &T) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    // Serialize to JSON
    let payload = serde_json::to_vec(message)?;
    let len = payload.len() as u32;

    // Validate length
    if len > MAX_MESSAGE_SIZE {
        return Err(BridgeError::NativeMessaging(format!(
            "Message too large: {} bytes (max {})",
            len, MAX_MESSAGE_SIZE
        )));
    }

    // Write length prefix
    writer.write_all(&len.to_le_bytes()).await?;

    // Write payload
    writer.write_all(&payload).await?;

    // Flush
    writer.flush().await?;

    Ok(())
}

/// Encode a message for native messaging (sync version for testing).
pub fn encode_message<T: Serialize>(message: &T) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(message)?;
    let len = payload.len() as u32;

    if len > MAX_MESSAGE_SIZE {
        return Err(BridgeError::NativeMessaging(format!(
            "Message too large: {} bytes (max {})",
            len, MAX_MESSAGE_SIZE
        )));
    }

    let mut result = Vec::with_capacity(4 + payload.len());
    result.extend_from_slice(&len.to_le_bytes());
    result.extend_from_slice(&payload);

    Ok(result)
}

/// Decode a message from native messaging format (sync version for testing).
pub fn decode_message<T: DeserializeOwned>(data: &[u8]) -> Result<T> {
    if data.len() < 4 {
        return Err(BridgeError::NativeMessaging(
            "Data too short for length prefix".into(),
        ));
    }

    let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

    if len > MAX_MESSAGE_SIZE {
        return Err(BridgeError::NativeMessaging(format!(
            "Message too large: {} bytes (max {})",
            len, MAX_MESSAGE_SIZE
        )));
    }

    if data.len() < 4 + len as usize {
        return Err(BridgeError::NativeMessaging(format!(
            "Data too short: expected {} bytes, got {}",
            4 + len,
            data.len()
        )));
    }

    serde_json::from_slice(&data[4..4 + len as usize]).map_err(BridgeError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::io::Cursor;
    use tokio::io::BufReader;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestMessage {
        text: String,
        number: i32,
    }

    #[test]
    fn test_encode_message() {
        let msg = TestMessage {
            text: "hello".into(),
            number: 42,
        };

        let encoded = encode_message(&msg).unwrap();

        // First 4 bytes are length
        let len = u32::from_le_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]);
        assert!(len > 0);
        assert_eq!(encoded.len(), 4 + len as usize);

        // Rest is JSON
        let json: serde_json::Value = serde_json::from_slice(&encoded[4..]).unwrap();
        assert_eq!(json["text"], "hello");
        assert_eq!(json["number"], 42);
    }

    #[test]
    fn test_decode_message() {
        let msg = TestMessage {
            text: "world".into(),
            number: 123,
        };

        let encoded = encode_message(&msg).unwrap();
        let decoded: TestMessage = decode_message(&encoded).unwrap();

        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_decode_message_too_short_for_prefix() {
        let result: Result<TestMessage> = decode_message(&[0, 0, 0]);
        assert!(matches!(result, Err(BridgeError::NativeMessaging(_))));
    }

    #[test]
    fn test_decode_message_too_short_for_payload() {
        // Length says 100 bytes, but only 10 provided
        let mut data = vec![100, 0, 0, 0]; // length = 100
        data.extend_from_slice(&[0; 10]); // only 10 bytes

        let result: Result<TestMessage> = decode_message(&data);
        assert!(matches!(result, Err(BridgeError::NativeMessaging(_))));
    }

    #[test]
    fn test_encode_message_too_large() {
        // Create a message that's too large
        let msg = TestMessage {
            text: "x".repeat(2 * 1024 * 1024), // 2 MB string
            number: 0,
        };

        let result = encode_message(&msg);
        assert!(matches!(result, Err(BridgeError::NativeMessaging(_))));
    }

    #[test]
    fn test_decode_message_claims_too_large() {
        // Length claims 2 MB
        let data = vec![0, 0, 32, 0]; // 2 MB in little-endian (0x00200000)
        let result: Result<TestMessage> = decode_message(&data);
        assert!(matches!(result, Err(BridgeError::NativeMessaging(_))));
    }

    #[tokio::test]
    async fn test_read_write_message() {
        let msg = TestMessage {
            text: "async test".into(),
            number: 999,
        };

        // Write to buffer
        let mut buf = Vec::new();
        write_message(&mut buf, &msg).await.unwrap();

        // Read from buffer
        let mut reader = BufReader::new(Cursor::new(buf));
        let decoded: TestMessage = read_message(&mut reader).await.unwrap();

        assert_eq!(decoded, msg);
    }

    #[tokio::test]
    async fn test_read_message_empty() {
        // Length = 0
        let data = vec![0, 0, 0, 0];
        let mut reader = BufReader::new(Cursor::new(data));

        let result: Result<TestMessage> = read_message(&mut reader).await;
        assert!(matches!(result, Err(BridgeError::NativeMessaging(_))));
    }

    #[tokio::test]
    async fn test_read_message_too_large() {
        // Length claims 2 MB
        let data = vec![0, 0, 32, 0];
        let mut reader = BufReader::new(Cursor::new(data));

        let result: Result<TestMessage> = read_message(&mut reader).await;
        assert!(matches!(result, Err(BridgeError::NativeMessaging(_))));
    }

    #[tokio::test]
    async fn test_write_message_too_large() {
        let msg = TestMessage {
            text: "x".repeat(2 * 1024 * 1024),
            number: 0,
        };

        let mut buf = Vec::new();
        let result = write_message(&mut buf, &msg).await;
        assert!(matches!(result, Err(BridgeError::NativeMessaging(_))));
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let messages = vec![
            TestMessage {
                text: "first".into(),
                number: 1,
            },
            TestMessage {
                text: "second".into(),
                number: 2,
            },
            TestMessage {
                text: "third".into(),
                number: 3,
            },
        ];

        // Write all messages
        let mut buf = Vec::new();
        for msg in &messages {
            write_message(&mut buf, msg).await.unwrap();
        }

        // Read all messages
        let mut reader = BufReader::new(Cursor::new(buf));
        for expected in &messages {
            let decoded: TestMessage = read_message(&mut reader).await.unwrap();
            assert_eq!(&decoded, expected);
        }
    }

    #[test]
    fn test_encode_decode_json_value() {
        let value = serde_json::json!({
            "type": "chat_message",
            "payload": {
                "session_id": "sess-001",
                "text": "Hello, world!"
            }
        });

        let encoded = encode_message(&value).unwrap();
        let decoded: serde_json::Value = decode_message(&encoded).unwrap();

        assert_eq!(decoded, value);
    }
}
