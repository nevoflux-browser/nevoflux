//! Message model and related types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// Error returned when parsing a message role from string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseMessageRoleError;

impl std::fmt::Display for ParseMessageRoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid message role")
    }
}

impl std::error::Error for ParseMessageRoleError {}

/// Error returned when parsing a content type from string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseContentTypeError;

impl std::fmt::Display for ParseContentTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid content type")
    }
}

impl std::error::Error for ParseContentTypeError {}

/// The role of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// Message from the user.
    #[default]
    User,
    /// Message from the assistant.
    Assistant,
    /// System message.
    System,
}

impl MessageRole {
    /// Convert the role to a string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        }
    }
}

impl FromStr for MessageRole {
    type Err = ParseMessageRoleError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "system" => Ok(MessageRole::System),
            _ => Err(ParseMessageRoleError),
        }
    }
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// The type of message content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    /// Plain text content.
    #[default]
    Text,
    /// Image content.
    Image,
    /// Tool use request.
    ToolUse,
    /// Tool execution result.
    ToolResult,
}

impl ContentType {
    /// Convert the content type to a string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Image => "image",
            ContentType::ToolUse => "tool_use",
            ContentType::ToolResult => "tool_result",
        }
    }
}

impl FromStr for ContentType {
    type Err = ParseContentTypeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(ContentType::Text),
            "image" => Ok(ContentType::Image),
            "tool_use" => Ok(ContentType::ToolUse),
            "tool_result" => Ok(ContentType::ToolResult),
            _ => Err(ParseContentTypeError),
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A message within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique identifier for the message.
    pub id: String,
    /// The session this message belongs to.
    pub session_id: String,
    /// The role of the message sender.
    pub role: MessageRole,
    /// The message content.
    pub content: String,
    /// The type of content.
    pub content_type: ContentType,
    /// Unix timestamp when the message was created.
    pub created_at: i64,
    /// Additional metadata as key-value pairs.
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Parameters for creating a new message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageParams {
    /// Optional ID (auto-generated if not provided).
    pub id: Option<String>,
    /// The session this message belongs to.
    pub session_id: String,
    /// The role of the message sender.
    pub role: MessageRole,
    /// The message content.
    pub content: String,
    /// The type of content (defaults to Text).
    pub content_type: Option<ContentType>,
    /// Additional metadata.
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl CreateMessageParams {
    /// Create new params with required fields.
    pub fn new(
        session_id: impl Into<String>,
        role: MessageRole,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: None,
            session_id: session_id.into(),
            role,
            content: content.into(),
            content_type: None,
            metadata: None,
        }
    }

    /// Set the message ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the content type.
    pub fn with_content_type(mut self, content_type: ContentType) -> Self {
        self.content_type = Some(content_type);
        self
    }

    /// Set the metadata.
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Parameters for listing messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListMessagesParams {
    /// Filter by session ID.
    pub session_id: String,
    /// Maximum number of results.
    pub limit: Option<u32>,
    /// Number of results to skip.
    pub offset: Option<u32>,
    /// Return messages created before this message ID.
    pub before_id: Option<String>,
    /// Return messages created after this message ID.
    pub after_id: Option<String>,
}

impl ListMessagesParams {
    /// Create new list params for a session.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            ..Default::default()
        }
    }

    /// Set the limit.
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the offset.
    pub fn with_offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Return messages before this message ID.
    pub fn before(mut self, message_id: impl Into<String>) -> Self {
        self.before_id = Some(message_id.into());
        self
    }

    /// Return messages after this message ID.
    pub fn after(mut self, message_id: impl Into<String>) -> Self {
        self.after_id = Some(message_id.into());
        self
    }
}

/// Generate a simple UUID v4-like identifier for messages.
#[cfg(test)]
fn message_uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Format: msg-{timestamp_hex}-{random_hex}
    let random_part: u64 = (timestamp as u64).wrapping_mul(6364136223846793005);
    format!("msg-{:016x}-{:08x}", timestamp as u64, random_part as u32)
}

/// Get the current Unix timestamp.
#[cfg(test)]
fn message_current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_role_as_str() {
        assert_eq!(MessageRole::User.as_str(), "user");
        assert_eq!(MessageRole::Assistant.as_str(), "assistant");
        assert_eq!(MessageRole::System.as_str(), "system");
    }

    #[test]
    fn test_message_role_from_str() {
        assert_eq!("user".parse::<MessageRole>(), Ok(MessageRole::User));
        assert_eq!("USER".parse::<MessageRole>(), Ok(MessageRole::User));
        assert_eq!(
            "assistant".parse::<MessageRole>(),
            Ok(MessageRole::Assistant)
        );
        assert_eq!(
            "Assistant".parse::<MessageRole>(),
            Ok(MessageRole::Assistant)
        );
        assert_eq!("system".parse::<MessageRole>(), Ok(MessageRole::System));
        assert_eq!("SYSTEM".parse::<MessageRole>(), Ok(MessageRole::System));
        assert!("invalid".parse::<MessageRole>().is_err());
    }

    #[test]
    fn test_message_role_default() {
        assert_eq!(MessageRole::default(), MessageRole::User);
    }

    #[test]
    fn test_message_role_display() {
        assert_eq!(format!("{}", MessageRole::User), "user");
        assert_eq!(format!("{}", MessageRole::Assistant), "assistant");
        assert_eq!(format!("{}", MessageRole::System), "system");
    }

    #[test]
    fn test_message_role_serialization() {
        let user = MessageRole::User;
        let json = serde_json::to_string(&user).unwrap();
        assert_eq!(json, "\"user\"");

        let assistant = MessageRole::Assistant;
        let json = serde_json::to_string(&assistant).unwrap();
        assert_eq!(json, "\"assistant\"");

        let system = MessageRole::System;
        let json = serde_json::to_string(&system).unwrap();
        assert_eq!(json, "\"system\"");
    }

    #[test]
    fn test_message_role_deserialization() {
        let user: MessageRole = serde_json::from_str("\"user\"").unwrap();
        assert_eq!(user, MessageRole::User);

        let assistant: MessageRole = serde_json::from_str("\"assistant\"").unwrap();
        assert_eq!(assistant, MessageRole::Assistant);

        let system: MessageRole = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(system, MessageRole::System);
    }

    #[test]
    fn test_content_type_as_str() {
        assert_eq!(ContentType::Text.as_str(), "text");
        assert_eq!(ContentType::Image.as_str(), "image");
        assert_eq!(ContentType::ToolUse.as_str(), "tool_use");
        assert_eq!(ContentType::ToolResult.as_str(), "tool_result");
    }

    #[test]
    fn test_content_type_from_str() {
        assert_eq!("text".parse::<ContentType>(), Ok(ContentType::Text));
        assert_eq!("TEXT".parse::<ContentType>(), Ok(ContentType::Text));
        assert_eq!("image".parse::<ContentType>(), Ok(ContentType::Image));
        assert_eq!("IMAGE".parse::<ContentType>(), Ok(ContentType::Image));
        assert_eq!("tool_use".parse::<ContentType>(), Ok(ContentType::ToolUse));
        assert_eq!("TOOL_USE".parse::<ContentType>(), Ok(ContentType::ToolUse));
        assert_eq!(
            "tool_result".parse::<ContentType>(),
            Ok(ContentType::ToolResult)
        );
        assert_eq!(
            "TOOL_RESULT".parse::<ContentType>(),
            Ok(ContentType::ToolResult)
        );
        assert!("invalid".parse::<ContentType>().is_err());
    }

    #[test]
    fn test_content_type_default() {
        assert_eq!(ContentType::default(), ContentType::Text);
    }

    #[test]
    fn test_content_type_display() {
        assert_eq!(format!("{}", ContentType::Text), "text");
        assert_eq!(format!("{}", ContentType::Image), "image");
        assert_eq!(format!("{}", ContentType::ToolUse), "tool_use");
        assert_eq!(format!("{}", ContentType::ToolResult), "tool_result");
    }

    #[test]
    fn test_content_type_serialization() {
        let text = ContentType::Text;
        let json = serde_json::to_string(&text).unwrap();
        assert_eq!(json, "\"text\"");

        let tool_use = ContentType::ToolUse;
        let json = serde_json::to_string(&tool_use).unwrap();
        assert_eq!(json, "\"tool_use\"");

        let tool_result = ContentType::ToolResult;
        let json = serde_json::to_string(&tool_result).unwrap();
        assert_eq!(json, "\"tool_result\"");
    }

    #[test]
    fn test_content_type_deserialization() {
        let text: ContentType = serde_json::from_str("\"text\"").unwrap();
        assert_eq!(text, ContentType::Text);

        let tool_use: ContentType = serde_json::from_str("\"tool_use\"").unwrap();
        assert_eq!(tool_use, ContentType::ToolUse);

        let tool_result: ContentType = serde_json::from_str("\"tool_result\"").unwrap();
        assert_eq!(tool_result, ContentType::ToolResult);
    }

    #[test]
    fn test_create_message_params_new() {
        let params = CreateMessageParams::new("session-123", MessageRole::User, "Hello");

        assert!(params.id.is_none());
        assert_eq!(params.session_id, "session-123");
        assert_eq!(params.role, MessageRole::User);
        assert_eq!(params.content, "Hello");
        assert!(params.content_type.is_none());
        assert!(params.metadata.is_none());
    }

    #[test]
    fn test_create_message_params_builder() {
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), serde_json::json!("value"));

        let params = CreateMessageParams::new("session-123", MessageRole::Assistant, "Response")
            .with_id("msg-123")
            .with_content_type(ContentType::ToolUse)
            .with_metadata(metadata.clone());

        assert_eq!(params.id, Some("msg-123".to_string()));
        assert_eq!(params.session_id, "session-123");
        assert_eq!(params.role, MessageRole::Assistant);
        assert_eq!(params.content, "Response");
        assert_eq!(params.content_type, Some(ContentType::ToolUse));
        assert_eq!(params.metadata, Some(metadata));
    }

    #[test]
    fn test_list_messages_params_new() {
        let params = ListMessagesParams::new("session-123");

        assert_eq!(params.session_id, "session-123");
        assert!(params.limit.is_none());
        assert!(params.offset.is_none());
        assert!(params.before_id.is_none());
        assert!(params.after_id.is_none());
    }

    #[test]
    fn test_list_messages_params_builder() {
        let params = ListMessagesParams::new("session-123")
            .with_limit(10)
            .with_offset(5)
            .before("msg-before")
            .after("msg-after");

        assert_eq!(params.session_id, "session-123");
        assert_eq!(params.limit, Some(10));
        assert_eq!(params.offset, Some(5));
        assert_eq!(params.before_id, Some("msg-before".to_string()));
        assert_eq!(params.after_id, Some("msg-after".to_string()));
    }

    #[test]
    fn test_message_uuid_v4_uniqueness() {
        let id1 = message_uuid_v4();
        let id2 = message_uuid_v4();
        // IDs should be different (with high probability)
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_message_uuid_v4_format() {
        let id = message_uuid_v4();
        assert!(id.starts_with("msg-"));
        assert!(id.len() > 20);
    }

    #[test]
    fn test_message_current_timestamp() {
        let ts = message_current_timestamp();
        // Should be a reasonable Unix timestamp (after year 2020)
        assert!(ts > 1577836800);
    }

    #[test]
    fn test_message_serialization() {
        let message = Message {
            id: "msg-123".to_string(),
            session_id: "sess-123".to_string(),
            role: MessageRole::User,
            content: "Hello".to_string(),
            content_type: ContentType::Text,
            created_at: 1234567890,
            metadata: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, message.id);
        assert_eq!(deserialized.session_id, message.session_id);
        assert_eq!(deserialized.role, message.role);
        assert_eq!(deserialized.content, message.content);
        assert_eq!(deserialized.content_type, message.content_type);
        assert_eq!(deserialized.created_at, message.created_at);
    }

    #[test]
    fn test_message_with_metadata_serialization() {
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), serde_json::json!("value"));
        metadata.insert("number".to_string(), serde_json::json!(42));

        let message = Message {
            id: "msg-123".to_string(),
            session_id: "sess-123".to_string(),
            role: MessageRole::Assistant,
            content: "Response".to_string(),
            content_type: ContentType::ToolResult,
            created_at: 1234567890,
            metadata: Some(metadata.clone()),
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.metadata, Some(metadata));
    }
}
