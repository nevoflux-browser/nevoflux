//! Session-level memory auto-extraction.
//!
//! Tracks user message counts and triggers LLM-based knowledge extraction
//! every N messages. Extracted knowledge is written directly to the knowledge
//! table with hot=1 for immediate availability.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Tracks user message count and controls extraction triggering.
pub struct SessionMemoryExtractor {
    /// User message count at last extraction.
    last_extracted_at: AtomicU32,
    /// Current user message count.
    user_message_count: AtomicU32,
    /// Trigger extraction every N user messages.
    extraction_interval: u32,
    /// Set to true when user manually called memory_create this turn.
    manual_memory_created: AtomicBool,
}

impl SessionMemoryExtractor {
    /// Create a new extractor.
    pub fn new(extraction_interval: u32) -> Self {
        Self {
            last_extracted_at: AtomicU32::new(0),
            user_message_count: AtomicU32::new(0),
            extraction_interval,
            manual_memory_created: AtomicBool::new(false),
        }
    }

    /// Increment user message counter.
    pub fn on_user_message(&self) {
        self.user_message_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Mark that user manually called memory_create this turn.
    pub fn mark_manual_create(&self) {
        self.manual_memory_created.store(true, Ordering::Relaxed);
    }

    /// Reset per-turn flags. Called at start of each turn.
    pub fn reset_turn_flags(&self) {
        self.manual_memory_created.store(false, Ordering::Relaxed);
    }

    /// Check if extraction should trigger.
    ///
    /// Returns true if extraction_interval messages have passed since last
    /// extraction AND no manual memory_create this turn.
    /// If true, updates last_extracted_at to current count.
    pub fn should_extract(&self) -> bool {
        if self.manual_memory_created.load(Ordering::Relaxed) {
            return false;
        }
        let current = self.user_message_count.load(Ordering::Relaxed);
        let last = self.last_extracted_at.load(Ordering::Relaxed);
        if current >= last + self.extraction_interval {
            self.last_extracted_at.store(current, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

/// A single extracted knowledge item from the LLM response.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExtractionItem {
    pub content: String,
    #[serde(default = "default_category")]
    pub category: String,
}

fn default_category() -> String {
    "user_preference".to_string()
}

/// Parse the LLM extraction response into items.
///
/// Expects a JSON array of objects with "content" and "category" fields.
/// Returns an empty vec on parse failure.
pub fn parse_extraction_response(response: &str) -> Vec<ExtractionItem> {
    let trimmed = response.trim();
    let json_str = if trimmed.starts_with('[') {
        trimmed.to_string()
    } else if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            trimmed[start..=end].to_string()
        } else {
            return vec![];
        }
    } else {
        return vec![];
    };

    serde_json::from_str(&json_str).unwrap_or_default()
}

/// The system prompt for knowledge extraction.
pub const EXTRACTION_SYSTEM_PROMPT: &str = r#"You are a knowledge extraction assistant. Analyze the recent conversation and extract information worth remembering across future sessions.

Extract these types of durable knowledge:

1. **user_preference** — User preferences and working style
   - Language, response style, tool preferences
   - Behavioral rules ("always do X", "never do Y")
   - Corrections the user made ("not that way, do it like this")

2. **workspace_context** — Facts about the user's environment and workflow
   - Tools, services, and platforms the user regularly uses
   - Team practices and processes (bug tracking, deployment, CI)
   - Important accounts, URLs, or resource locations

3. **tool_optimization** — Tool usage patterns learned
   - Which tools work/fail on which sites
   - Effective command patterns or workarounds
   - Site-specific selectors or interaction strategies

4. **error_pattern** — Recurring errors and proven fixes
   - Errors that were fixed with a specific approach
   - Approaches that failed and should not be tried again
   - User corrections on how to handle specific situations

Do NOT extract:
- Temporary task details (the specific bug being fixed right now)
- Information already in the existing knowledge list below
- Greetings, small talk, or routine exchanges
- One-off facts unlikely to be useful in future sessions

Each extracted item must be a self-contained sentence that is useful without the original conversation context.

Return a JSON array. Each element has:
- "content": the knowledge to remember (one clear, specific sentence)
- "category": one of "user_preference", "workspace_context", "tool_optimization", "error_pattern"

If nothing is worth extracting, return an empty array: []"#;

/// Format the user message for the extraction LLM call.
pub fn build_extraction_user_prompt(
    existing_knowledge: &[String],
    recent_messages: &[(String, String)],
) -> String {
    let mut parts = Vec::new();

    parts.push("Existing knowledge (do not duplicate):".to_string());
    if existing_knowledge.is_empty() {
        parts.push("(none)".to_string());
    } else {
        for k in existing_knowledge {
            parts.push(format!("- {}", k));
        }
    }

    parts.push(String::new());
    parts.push("Recent conversation:".to_string());
    for (role, content) in recent_messages {
        let truncated = if content.len() > 500 {
            format!("{}...", &content[..content.floor_char_boundary(497)])
        } else {
            content.clone()
        };
        parts.push(format!("{}: {}", role, truncated));
    }

    parts.join("\n")
}

use crate::config::AgentConfig;
use crate::context::ContextMessage;
use crate::error::Result;
use nevoflux_storage::{Database, KnowledgeRepository};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Execute session memory extraction using the current LLM provider.
///
/// Spawned as a background task — errors are logged, not propagated to the user.
pub async fn extract_session_memories(
    config: Arc<AgentConfig>,
    database: Arc<Database>,
    recent_messages: Vec<ContextMessage>,
) -> Result<usize> {
    if recent_messages.is_empty() {
        return Ok(0);
    }

    // Get provider configuration.
    // If the active provider is ACP (GeminiCli, ClaudeCode), get_summarization_provider
    // falls back to a non-ACP provider. In that case, use the fallback provider's default
    // model instead of the active model (which belongs to the ACP provider).
    let active_model = config.llm.active_model().unwrap_or("gpt-4o-mini");
    let (provider, api_key) = crate::context::get_summarization_provider(&config, active_model)?;

    let active_provider = config
        .llm
        .active_provider()
        .and_then(|p| p.parse::<nevoflux_llm::ProviderType>().ok());
    let is_fallback = active_provider.map(|ap| ap != provider).unwrap_or(false);
    let model = if is_fallback {
        nevoflux_llm::default_model_for(provider)
    } else {
        active_model
    };
    let base_url = if is_fallback {
        // Don't use active provider's base_url when falling back
        None
    } else {
        config.llm.active_base_url()
    };

    // Gather existing hot knowledge to avoid duplicates
    let knowledge_repo = KnowledgeRepository::new(&database);
    let existing: Vec<String> = knowledge_repo
        .list_hot()
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.hot_summary.unwrap_or(e.summary))
        .collect();

    // Format messages for the prompt
    let msg_pairs: Vec<(String, String)> = recent_messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| (m.role.clone(), m.content.clone()))
        .collect();

    let user_prompt = build_extraction_user_prompt(&existing, &msg_pairs);

    // Call LLM
    let request = crate::wasm::llm::LlmChatRequest {
        messages: vec![crate::wasm::llm::LlmMessage::user(user_prompt)],
        system: Some(EXTRACTION_SYSTEM_PROMPT.into()),
        temperature: Some(0.3),
        max_tokens: Some(500),
        tools: None,
    };

    debug!(
        "Extracting session memories using model={}, provider={:?}",
        model, provider
    );

    let response =
        crate::wasm::llm::execute_llm_chat(provider, &api_key, model, request, base_url).await?;

    // Parse response
    let items = parse_extraction_response(&response.content);
    if items.is_empty() {
        debug!("Session extraction: nothing worth extracting");
        return Ok(0);
    }

    // Write each item to knowledge table (hot=1)
    let mut written = 0;
    for item in &items {
        if item.content.trim().is_empty() {
            continue;
        }

        let summary = if item.content.len() > 120 {
            let boundary = item.content.floor_char_boundary(117);
            format!("{}...", &item.content[..boundary])
        } else {
            item.content.clone()
        };

        let params = nevoflux_storage::CreateKnowledgeParams {
            category: item.category.clone(),
            summary: summary.clone(),
            details: item.content.clone(),
            source_type: Some("auto_extraction".into()),
            priority: Some("medium".into()),
            tags: Some("[\"auto_extracted\"]".into()),
            privacy_level: Some("internal".into()),
            ..Default::default()
        };

        match knowledge_repo.create(params) {
            Ok(entry) => {
                let id = entry.id.clone();
                if let Err(e) = knowledge_repo.update_status(&id, "validated") {
                    warn!("Failed to validate extracted knowledge {}: {}", id, e);
                    continue;
                }
                if let Err(e) = knowledge_repo.mark_hot(&id, &summary) {
                    warn!("Failed to mark extracted knowledge hot {}: {}", id, e);
                    continue;
                }
                written += 1;
            }
            Err(e) => {
                warn!("Failed to create extracted knowledge: {}", e);
            }
        }
    }

    if written > 0 {
        info!("Session extraction: wrote {} knowledge entries", written);
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extractor_skipped_under_interval() {
        let ext = SessionMemoryExtractor::new(5);
        ext.on_user_message();
        ext.on_user_message();
        ext.on_user_message();
        assert!(!ext.should_extract());
    }

    #[test]
    fn test_extractor_triggers_at_interval() {
        let ext = SessionMemoryExtractor::new(5);
        for _ in 0..5 {
            ext.on_user_message();
        }
        assert!(ext.should_extract());
    }

    #[test]
    fn test_extractor_skipped_on_manual_create() {
        let ext = SessionMemoryExtractor::new(5);
        for _ in 0..5 {
            ext.on_user_message();
        }
        ext.mark_manual_create();
        assert!(!ext.should_extract());
    }

    #[test]
    fn test_extractor_resets_after_extraction() {
        let ext = SessionMemoryExtractor::new(5);
        for _ in 0..5 {
            ext.on_user_message();
        }
        assert!(ext.should_extract());
        assert!(!ext.should_extract());

        for _ in 0..5 {
            ext.on_user_message();
        }
        assert!(ext.should_extract());
    }

    #[test]
    fn test_extractor_reset_turn_flags() {
        let ext = SessionMemoryExtractor::new(5);
        for _ in 0..5 {
            ext.on_user_message();
        }
        ext.mark_manual_create();
        assert!(!ext.should_extract());

        ext.reset_turn_flags();
        assert!(ext.should_extract());
    }

    #[test]
    fn test_parse_extraction_result() {
        let json = r#"[{"content": "User prefers dark mode", "category": "user_preference"}]"#;
        let items = parse_extraction_response(json);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].content, "User prefers dark mode");
        assert_eq!(items[0].category, "user_preference");
    }

    #[test]
    fn test_parse_extraction_result_empty() {
        let items = parse_extraction_response("[]");
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_extraction_result_invalid() {
        let items = parse_extraction_response("this is not json");
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_extraction_result_markdown_wrapped() {
        let response = "Here are the extracted items:\n```json\n[{\"content\": \"test\", \"category\": \"user_preference\"}]\n```";
        let items = parse_extraction_response(response);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_extraction_prompt_contains_existing() {
        let existing = vec!["User prefers dark mode".to_string()];
        let messages = vec![
            ("user".to_string(), "I like Rust".to_string()),
            ("assistant".to_string(), "Great choice!".to_string()),
        ];
        let prompt = build_extraction_user_prompt(&existing, &messages);
        assert!(prompt.contains("User prefers dark mode"));
        assert!(prompt.contains("user: I like Rust"));
        assert!(prompt.contains("assistant: Great choice!"));
    }

    #[test]
    fn test_extraction_prompt_truncates_long_messages() {
        let long_msg = "x".repeat(1000);
        let messages = vec![("tool".to_string(), long_msg)];
        let prompt = build_extraction_user_prompt(&[], &messages);
        assert!(prompt.len() < 800);
        assert!(prompt.contains("..."));
    }
}
