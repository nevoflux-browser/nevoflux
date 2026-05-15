//! Context builder for LLM requests.

use crate::config::ContextConfig;
use nevoflux_storage::Message;
use serde::{Deserialize, Serialize};

/// Token budget for context building.
#[derive(Debug, Clone, Copy)]
pub struct TokenBudget {
    /// Total tokens available.
    pub total: u32,
    /// Tokens reserved for system prompt.
    pub for_system_prompt: u32,
    /// Tokens available for history.
    pub for_history: u32,
    /// Tokens reserved for response.
    pub for_response: u32,
}

impl TokenBudget {
    /// Calculate token budget for a model.
    pub fn for_model(context_window: u32, max_output_tokens: u32, config: &ContextConfig) -> Self {
        let for_history = context_window
            .saturating_sub(config.system_prompt_reserve)
            .saturating_sub(max_output_tokens)
            .saturating_sub(config.safety_margin);

        Self {
            total: context_window,
            for_system_prompt: config.system_prompt_reserve,
            for_history,
            for_response: max_output_tokens,
        }
    }
}

/// Built context ready for LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// System prompt.
    pub system_prompt: String,
    /// Conversation history.
    pub messages: Vec<ContextMessage>,
    /// Available tools.
    pub tools: Vec<ToolContext>,
    /// Current page information.
    pub current_page: Option<PageContext>,
    /// Active skills.
    pub skills: Vec<SkillContext>,
    /// Estimated token count.
    pub estimated_tokens: u32,
}

/// A message in the context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    /// Role (user, assistant, system).
    pub role: String,
    /// Message content.
    pub content: String,
}

impl ContextMessage {
    /// Create from a storage message.
    pub fn from_message(message: &Message) -> Self {
        Self {
            role: message.role.as_str().to_string(),
            content: message.content.clone(),
        }
    }
}

/// Tool information for context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContext {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
}

/// Current page information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageContext {
    /// Page URL.
    pub url: String,
    /// Page title.
    pub title: String,
    /// Selected text (if any).
    pub selected_text: Option<String>,
}

/// Skill information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContext {
    /// Skill name.
    pub name: String,
    /// Skill prompt content.
    pub content: String,
}

/// Builder for constructing LLM context.
pub struct ContextBuilder {
    #[allow(dead_code)]
    config: ContextConfig,
    system_prompt: String,
    messages: Vec<ContextMessage>,
    tools: Vec<ToolContext>,
    current_page: Option<PageContext>,
    skills: Vec<SkillContext>,
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextBuilder {
    /// Create a new context builder.
    pub fn new() -> Self {
        Self {
            config: ContextConfig::default(),
            system_prompt: String::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            current_page: None,
            skills: Vec::new(),
        }
    }

    /// Create with configuration.
    pub fn with_config(config: ContextConfig) -> Self {
        Self {
            config,
            system_prompt: String::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            current_page: None,
            skills: Vec::new(),
        }
    }

    /// Set the system prompt.
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Add a message to the context.
    pub fn add_message(mut self, role: impl Into<String>, content: impl Into<String>) -> Self {
        self.messages.push(ContextMessage {
            role: role.into(),
            content: content.into(),
        });
        self
    }

    /// Add messages from storage.
    pub fn add_messages(mut self, messages: &[Message]) -> Self {
        for message in messages {
            self.messages.push(ContextMessage::from_message(message));
        }
        self
    }

    /// Add recent messages with token budget.
    pub fn add_recent_messages(mut self, messages: &[Message], max_messages: usize) -> Self {
        // Take the most recent messages up to max_messages
        let start = messages.len().saturating_sub(max_messages);
        for message in &messages[start..] {
            self.messages.push(ContextMessage::from_message(message));
        }
        self
    }

    /// Add a tool.
    pub fn add_tool(mut self, name: impl Into<String>, description: impl Into<String>) -> Self {
        self.tools.push(ToolContext {
            name: name.into(),
            description: description.into(),
        });
        self
    }

    /// Set current page information.
    pub fn current_page(mut self, url: impl Into<String>, title: impl Into<String>) -> Self {
        self.current_page = Some(PageContext {
            url: url.into(),
            title: title.into(),
            selected_text: None,
        });
        self
    }

    /// Set current page with selected text.
    pub fn current_page_with_selection(
        mut self,
        url: impl Into<String>,
        title: impl Into<String>,
        selected: impl Into<String>,
    ) -> Self {
        self.current_page = Some(PageContext {
            url: url.into(),
            title: title.into(),
            selected_text: Some(selected.into()),
        });
        self
    }

    /// Add a skill.
    pub fn add_skill(mut self, name: impl Into<String>, content: impl Into<String>) -> Self {
        self.skills.push(SkillContext {
            name: name.into(),
            content: content.into(),
        });
        self
    }

    /// Get current messages for inspection.
    pub fn messages(&self) -> &[ContextMessage] {
        &self.messages
    }

    /// Get estimated token count.
    pub fn current_estimated_tokens(&self) -> u32 {
        self.estimate_tokens()
    }

    /// Replace messages with compressed set (summary + recent).
    pub fn with_compressed_messages(
        mut self,
        summary: String,
        recent: Vec<ContextMessage>,
    ) -> Self {
        self.messages = vec![ContextMessage {
            role: "system".to_string(),
            content: format!("[Conversation summary]\n{}", summary),
        }];
        self.messages.extend(recent);
        self
    }

    /// Build the context.
    pub fn build(self) -> Context {
        let estimated_tokens = self.estimate_tokens();

        Context {
            system_prompt: self.system_prompt,
            messages: self.messages,
            tools: self.tools,
            current_page: self.current_page,
            skills: self.skills,
            estimated_tokens,
        }
    }

    /// Estimate token count (rough approximation: 4 chars = 1 token).
    fn estimate_tokens(&self) -> u32 {
        let mut chars = 0;

        chars += self.system_prompt.len();

        for msg in &self.messages {
            chars += msg.role.len();
            chars += msg.content.len();
        }

        for tool in &self.tools {
            chars += tool.name.len();
            chars += tool.description.len();
        }

        if let Some(page) = &self.current_page {
            chars += page.url.len();
            chars += page.title.len();
            if let Some(selected) = &page.selected_text {
                chars += selected.len();
            }
        }

        for skill in &self.skills {
            chars += skill.name.len();
            chars += skill.content.len();
        }

        (chars / 4) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nevoflux_storage::MessageRole;

    #[test]
    fn test_token_budget_calculation() {
        let config = ContextConfig::default();
        let budget = TokenBudget::for_model(128_000, 8_192, &config);

        assert_eq!(budget.total, 128_000);
        assert_eq!(budget.for_system_prompt, 2000);
        assert_eq!(budget.for_response, 8_192);
        // for_history = 128000 - 2000 - 8192 - 500 = 117308
        assert_eq!(budget.for_history, 117_308);
    }

    #[test]
    fn test_context_builder_new() {
        let context = ContextBuilder::new().build();

        assert!(context.system_prompt.is_empty());
        assert!(context.messages.is_empty());
        assert!(context.tools.is_empty());
    }

    #[test]
    fn test_context_builder_system_prompt() {
        let context = ContextBuilder::new()
            .system_prompt("You are a helpful assistant.")
            .build();

        assert_eq!(context.system_prompt, "You are a helpful assistant.");
    }

    #[test]
    fn test_context_builder_add_message() {
        let context = ContextBuilder::new()
            .add_message("user", "Hello!")
            .add_message("assistant", "Hi there!")
            .build();

        assert_eq!(context.messages.len(), 2);
        assert_eq!(context.messages[0].role, "user");
        assert_eq!(context.messages[1].role, "assistant");
    }

    #[test]
    fn test_context_builder_add_messages_from_storage() {
        let messages = vec![
            Message {
                id: "msg-1".to_string(),
                session_id: "sess-1".to_string(),
                role: MessageRole::User,
                content: "Hello".to_string(),
                content_type: nevoflux_storage::ContentType::Text,
                created_at: 0,
                metadata: None,
            },
            Message {
                id: "msg-2".to_string(),
                session_id: "sess-1".to_string(),
                role: MessageRole::Assistant,
                content: "Hi".to_string(),
                content_type: nevoflux_storage::ContentType::Text,
                created_at: 0,
                metadata: None,
            },
        ];

        let context = ContextBuilder::new().add_messages(&messages).build();

        assert_eq!(context.messages.len(), 2);
    }

    #[test]
    fn test_context_builder_add_recent_messages() {
        let messages: Vec<Message> = (0..10)
            .map(|i| Message {
                id: format!("msg-{}", i),
                session_id: "sess-1".to_string(),
                role: MessageRole::User,
                content: format!("Message {}", i),
                content_type: nevoflux_storage::ContentType::Text,
                created_at: i,
                metadata: None,
            })
            .collect();

        let context = ContextBuilder::new()
            .add_recent_messages(&messages, 3)
            .build();

        assert_eq!(context.messages.len(), 3);
        assert_eq!(context.messages[0].content, "Message 7");
        assert_eq!(context.messages[2].content, "Message 9");
    }

    #[test]
    fn test_context_builder_add_tool() {
        let context = ContextBuilder::new()
            .add_tool("bash", "Execute shell commands")
            .add_tool("read_file", "Read file contents")
            .build();

        assert_eq!(context.tools.len(), 2);
        assert_eq!(context.tools[0].name, "bash");
    }

    #[test]
    fn test_context_builder_current_page() {
        let context = ContextBuilder::new()
            .current_page("https://example.com", "Example Page")
            .build();

        assert!(context.current_page.is_some());
        let page = context.current_page.unwrap();
        assert_eq!(page.url, "https://example.com");
        assert_eq!(page.title, "Example Page");
    }

    #[test]
    fn test_context_builder_current_page_with_selection() {
        let context = ContextBuilder::new()
            .current_page_with_selection("https://example.com", "Page", "Selected text")
            .build();

        let page = context.current_page.unwrap();
        assert_eq!(page.selected_text, Some("Selected text".to_string()));
    }

    #[test]
    fn test_context_builder_add_skill() {
        let context = ContextBuilder::new()
            .add_skill("code-review", "Review code for best practices")
            .build();

        assert_eq!(context.skills.len(), 1);
        assert_eq!(context.skills[0].name, "code-review");
    }

    #[test]
    fn test_context_builder_estimated_tokens() {
        let context = ContextBuilder::new()
            .system_prompt("System prompt here") // 18 chars
            .add_message("user", "Hello!") // 4 + 6 = 10 chars
            .build();

        // Total chars = 28, estimated tokens = 28 / 4 = 7
        assert_eq!(context.estimated_tokens, 7);
    }

    #[test]
    fn test_context_serialization() {
        let context = ContextBuilder::new()
            .system_prompt("Test")
            .add_message("user", "Hi")
            .build();

        let json = serde_json::to_string(&context).unwrap();
        let decoded: Context = serde_json::from_str(&json).unwrap();

        assert_eq!(context.system_prompt, decoded.system_prompt);
        assert_eq!(context.messages.len(), decoded.messages.len());
    }

    #[test]
    fn test_context_builder_messages() {
        let builder = ContextBuilder::new()
            .add_message("user", "Hello")
            .add_message("assistant", "Hi there");

        let messages = builder.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
    }

    #[test]
    fn test_context_builder_current_estimated_tokens() {
        let builder = ContextBuilder::new()
            .system_prompt("System prompt") // 13 chars
            .add_message("user", "Hello"); // 4 + 5 = 9 chars

        // Total chars = 22, estimated tokens = 22 / 4 = 5
        assert_eq!(builder.current_estimated_tokens(), 5);
    }

    #[test]
    fn test_context_builder_with_compressed_messages() {
        let summary = "User asked about weather, assistant provided forecast.".to_string();
        let recent = vec![
            ContextMessage {
                role: "user".to_string(),
                content: "What's the temperature?".to_string(),
            },
            ContextMessage {
                role: "assistant".to_string(),
                content: "It's 72°F.".to_string(),
            },
        ];

        let context = ContextBuilder::new()
            .add_message("user", "Old message 1")
            .add_message("assistant", "Old response 1")
            .with_compressed_messages(summary.clone(), recent)
            .build();

        // Should have 3 messages: summary + 2 recent
        assert_eq!(context.messages.len(), 3);
        assert_eq!(context.messages[0].role, "system");
        assert!(context.messages[0]
            .content
            .contains("[Conversation summary]"));
        assert!(context.messages[0].content.contains(&summary));
        assert_eq!(context.messages[1].role, "user");
        assert_eq!(context.messages[2].role, "assistant");
    }
}
