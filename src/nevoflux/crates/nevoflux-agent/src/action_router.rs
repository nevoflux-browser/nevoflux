/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Action Router
//!
//! Maps action_id from UI events to handler functions for quick callback execution

use anyhow::Result;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Action handler function type
///
/// Takes session_id, action_id, and form_data, returns a result
pub type ActionHandler = Arc<
    dyn Fn(
            String,
            String,
            HashMap<String, serde_json::Value>,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send>>
        + Send
        + Sync,
>;

/// Action router for handling UI events
pub struct ActionRouter {
    handlers: Arc<RwLock<HashMap<String, ActionHandler>>>,
}

impl ActionRouter {
    /// Create a new action router
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an action handler
    pub async fn register<F, Fut>(&self, action_id: String, handler: F)
    where
        F: Fn(String, String, HashMap<String, serde_json::Value>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<serde_json::Value>> + Send + 'static,
    {
        let handler: ActionHandler = Arc::new(move |session_id, action_id, form_data| {
            Box::pin(handler(session_id, action_id, form_data))
        });

        let mut handlers = self.handlers.write().await;
        handlers.insert(action_id.clone(), handler);
        debug!("Registered action handler: {}", action_id);
    }

    /// Unregister an action handler
    pub async fn unregister(&self, action_id: &str) -> bool {
        let mut handlers = self.handlers.write().await;
        let removed = handlers.remove(action_id).is_some();

        if removed {
            debug!("Unregistered action handler: {}", action_id);
        }

        removed
    }

    /// Handle an action
    pub async fn handle(
        &self,
        session_id: String,
        action_id: String,
        form_data: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let handlers = self.handlers.read().await;

        if let Some(handler) = handlers.get(&action_id) {
            debug!("Handling action: {} for session: {}", action_id, session_id);

            let handler = Arc::clone(handler);
            drop(handlers); // Release lock before executing handler

            handler(session_id, action_id, form_data).await
        } else {
            warn!("No handler registered for action: {}", action_id);
            anyhow::bail!("Unknown action: {}", action_id)
        }
    }

    /// Check if an action is registered
    pub async fn has_handler(&self, action_id: &str) -> bool {
        let handlers = self.handlers.read().await;
        handlers.contains_key(action_id)
    }

    /// List all registered actions
    pub async fn list_actions(&self) -> Vec<String> {
        let handlers = self.handlers.read().await;
        handlers.keys().cloned().collect()
    }

    /// Clear all handlers
    pub async fn clear(&self) {
        let mut handlers = self.handlers.write().await;
        handlers.clear();
        debug!("Cleared all action handlers");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_handle() {
        let router = ActionRouter::new();

        // Register a test action
        router
            .register(
                "test_action".to_string(),
                |_session_id, action_id, form_data| async move {
                    Ok(serde_json::json!({
                        "action": action_id,
                        "form_data": form_data,
                        "result": "success"
                    }))
                },
            )
            .await;

        // Handle the action
        let mut form_data = HashMap::new();
        form_data.insert("key".to_string(), serde_json::json!("value"));

        let result = router
            .handle(
                "session-1".to_string(),
                "test_action".to_string(),
                form_data,
            )
            .await
            .unwrap();

        assert_eq!(result["result"], "success");
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let router = ActionRouter::new();

        let result = router
            .handle(
                "session-1".to_string(),
                "unknown_action".to_string(),
                HashMap::new(),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unregister() {
        let router = ActionRouter::new();

        router
            .register(
                "test_action".to_string(),
                |_session_id, _action_id, _form_data| async move { Ok(serde_json::json!({})) },
            )
            .await;

        assert!(router.has_handler("test_action").await);

        router.unregister("test_action").await;

        assert!(!router.has_handler("test_action").await);
    }
}
