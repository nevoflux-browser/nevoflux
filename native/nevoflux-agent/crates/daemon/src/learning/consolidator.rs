//! Knowledge consolidation (Auto-Dream).
//!
//! When hot knowledge entries approach capacity limits, uses LLM to
//! merge/deduplicate entries. Old entries are archived (unmark_hot),
//! consolidated entries are created as new hot entries.

use crate::config::AgentConfig;
use crate::error::Result;
use nevoflux_storage::{Database, KnowledgeRepository};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Result of a consolidation pass.
#[derive(Debug)]
pub struct ConsolidationResult {
    pub category: String,
    pub original_count: usize,
    pub consolidated_count: usize,
    pub archived_count: usize,
}

/// A single consolidated item from the LLM response.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ConsolidationItem {
    pub content: String,
}

/// Checks capacity and triggers LLM-based consolidation when needed.
pub struct KnowledgeConsolidator {
    /// Trigger when hot count >= limit * threshold_ratio
    threshold_ratio: f64,
}

impl KnowledgeConsolidator {
    /// Create a new consolidator.
    pub fn new(threshold_ratio: f64) -> Self {
        Self { threshold_ratio }
    }

    /// Check if any category needs consolidation.
    ///
    /// Returns the first category at or above the threshold, or None.
    pub fn category_needing_consolidation(
        &self,
        database: &Database,
        hot_limits: &[(String, usize)],
    ) -> Option<(String, usize)> {
        let repo = KnowledgeRepository::new(database);
        for (category, limit) in hot_limits {
            let threshold = (*limit as f64 * self.threshold_ratio) as usize;
            let count = repo.count_hot_by_category(category).unwrap_or(0);
            if count >= threshold {
                return Some((category.clone(), *limit));
            }
        }
        None
    }

    /// Target count after consolidation (70% of limit).
    pub fn target_count(limit: usize) -> usize {
        (limit as f64 * 0.7) as usize
    }
}

/// The system prompt for knowledge consolidation.
const CONSOLIDATION_SYSTEM_PROMPT: &str = r#"You are a knowledge consolidation assistant. Review the knowledge entries and produce a refined, deduplicated list.

Rules:
- Merge entries with similar or overlapping meaning into one
- Remove entries that are outdated or contradicted by newer ones
- Preserve all unique, non-redundant information
- Each entry should be one clear, self-contained sentence

Return a JSON array of objects, each with:
- "content": the consolidated knowledge (one sentence)

If all entries are already unique and non-redundant, return them unchanged.
Return only the JSON array, no other text."#;

/// Parse the LLM consolidation response.
pub fn parse_consolidation_response(response: &str) -> Vec<ConsolidationItem> {
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

/// Consolidate hot knowledge entries for a category using LLM.
///
/// 1. Query all hot entries for the category
/// 2. Call LLM to merge/deduplicate
/// 3. Create new consolidated entries (hot=1)
/// 4. Archive old entries (unmark_hot) — ONLY after new ones succeed
pub async fn consolidate_category(
    config: Arc<AgentConfig>,
    database: Arc<Database>,
    category: &str,
    target_count: usize,
) -> Result<ConsolidationResult> {
    let repo = KnowledgeRepository::new(&database);

    // 1. Get all hot entries for this category
    let hot_entries = repo.list_hot_by_category(category)?;
    let original_count = hot_entries.len();

    if original_count == 0 {
        return Ok(ConsolidationResult {
            category: category.to_string(),
            original_count: 0,
            consolidated_count: 0,
            archived_count: 0,
        });
    }

    // 2. Build prompt
    let entries_list: Vec<String> = hot_entries
        .iter()
        .map(|e| {
            let summary = e.hot_summary.as_deref().unwrap_or(&e.summary);
            format!("- {}", summary)
        })
        .collect();

    let user_prompt = format!(
        "Target: keep at most {} entries.\n\nCurrent entries for category \"{}\":\n{}",
        target_count,
        category,
        entries_list.join("\n")
    );

    // 3. Call LLM (with ACP fallback — same logic as session_extractor)
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
        None
    } else {
        config.llm.active_base_url()
    };

    let request = crate::wasm::llm::LlmChatRequest {
        messages: vec![crate::wasm::llm::LlmMessage::user(user_prompt)],
        system: Some(CONSOLIDATION_SYSTEM_PROMPT.into()),
        temperature: Some(0.3),
        max_tokens: Some(1000),
        tools: None,
    };

    debug!(
        "Consolidating {} hot entries for category '{}', target={}",
        original_count, category, target_count
    );

    let response =
        crate::wasm::llm::execute_llm_chat(provider, &api_key, model, request, base_url).await?;

    // 4. Parse response
    let mut items = parse_consolidation_response(&response.content);
    if items.is_empty() {
        warn!("Consolidation returned empty result, skipping");
        return Ok(ConsolidationResult {
            category: category.to_string(),
            original_count,
            consolidated_count: 0,
            archived_count: 0,
        });
    }

    // Cap at target_count
    items.truncate(target_count);

    // 5. Create new consolidated entries FIRST (safety: before archiving)
    let mut new_ids = Vec::new();
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
            category: category.to_string(),
            summary: summary.clone(),
            details: item.content.clone(),
            source_type: Some("consolidation".into()),
            priority: Some("medium".into()),
            tags: Some("[\"consolidated\"]".into()),
            privacy_level: Some("internal".into()),
            ..Default::default()
        };

        match repo.create(params) {
            Ok(entry) => {
                let id = entry.id.clone();
                if let Err(e) = repo.update_status(&id, "validated") {
                    warn!("Failed to validate consolidated entry {}: {}", id, e);
                    continue;
                }
                if let Err(e) = repo.mark_hot(&id, &summary) {
                    warn!("Failed to mark consolidated entry hot {}: {}", id, e);
                    continue;
                }
                new_ids.push(id);
            }
            Err(e) => {
                warn!("Failed to create consolidated entry: {}", e);
            }
        }
    }

    // 6. Archive old entries ONLY if new ones were created successfully
    let mut archived_count = 0;
    if !new_ids.is_empty() {
        for old_entry in &hot_entries {
            if let Err(e) = repo.unmark_hot(&old_entry.id) {
                warn!("Failed to archive old entry {}: {}", old_entry.id, e);
            } else {
                archived_count += 1;
            }
        }
    }

    let consolidated_count = new_ids.len();
    info!(
        "Consolidation complete for '{}': {} → {} entries ({} archived)",
        category, original_count, consolidated_count, archived_count
    );

    Ok(ConsolidationResult {
        category: category.to_string(),
        original_count,
        consolidated_count,
        archived_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_consolidate_under_threshold() {
        let db = Database::open_in_memory().unwrap();
        let consolidator = KnowledgeConsolidator::new(0.8);
        let hot_limits = vec![
            ("user_preference".to_string(), 10),
            ("tool_optimization".to_string(), 10),
            ("site_interaction".to_string(), 15),
        ];

        let result = consolidator.category_needing_consolidation(&db, &hot_limits);
        assert!(result.is_none());
    }

    #[test]
    fn test_should_consolidate_at_threshold() {
        let db = Database::open_in_memory().unwrap();
        let repo = KnowledgeRepository::new(&db);

        for i in 0..8 {
            let params = nevoflux_storage::CreateKnowledgeParams {
                category: "user_preference".to_string(),
                summary: format!("Pref {}", i),
                details: format!("Details {}", i),
                ..Default::default()
            };
            let entry = repo.create(params).unwrap();
            repo.update_status(&entry.id, "validated").unwrap();
            repo.mark_hot(&entry.id, &format!("Pref {}", i)).unwrap();
        }

        let consolidator = KnowledgeConsolidator::new(0.8);
        let hot_limits = vec![
            ("user_preference".to_string(), 10),
            ("tool_optimization".to_string(), 10),
            ("site_interaction".to_string(), 15),
        ];

        let result = consolidator.category_needing_consolidation(&db, &hot_limits);
        assert!(result.is_some());
        let (cat, limit) = result.unwrap();
        assert_eq!(cat, "user_preference");
        assert_eq!(limit, 10);
    }

    #[test]
    fn test_target_count() {
        assert_eq!(KnowledgeConsolidator::target_count(10), 7);
        assert_eq!(KnowledgeConsolidator::target_count(15), 10);
        assert_eq!(KnowledgeConsolidator::target_count(0), 0);
    }

    #[test]
    fn test_parse_consolidation_result() {
        let json = r#"[{"content": "User prefers dark mode"}, {"content": "Uses Rust daily"}]"#;
        let items = parse_consolidation_response(json);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].content, "User prefers dark mode");
        assert_eq!(items[1].content, "Uses Rust daily");
    }

    #[test]
    fn test_parse_consolidation_result_empty() {
        let items = parse_consolidation_response("[]");
        assert!(items.is_empty());

        let items = parse_consolidation_response("not json");
        assert!(items.is_empty());
    }
}
