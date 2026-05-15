// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BrowserBridge` — async trait for dispatching a single Actor call.
//!
//! The real implementation (`RealBrowserBridge`) wraps the existing
//! `BrowserContext` + `BrowserRequest` + oneshot channel pattern used
//! by `BrowserTool::execute` in `tools.rs`. A mock implementation is
//! used in unit tests so the strategy executor can be exercised
//! without a running browser.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use nevoflux_protocol::{BrowserToolAction, BrowserToolError};
use serde_json::Value;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::agent::browser_input::error::BrowserInputError;
use crate::wasm::services::{BrowserContext, BrowserRequest, BrowserResponse};

/// Abstraction over "send one action to the browser and await its response".
///
/// Implementations handle the messaging channel and timeout. The
/// caller only cares about the deserialized result JSON or an error.
#[async_trait]
pub trait BrowserBridge: Send + Sync {
    async fn call_action(
        &self,
        action: BrowserToolAction,
        params: Value,
        tab_id: Option<i64>,
    ) -> Result<Value, BrowserInputError>;
}

/// Real implementation backed by an `Arc<BrowserContext>`.
///
/// Sends the request on the context's mpsc channel and awaits the
/// response on a fresh oneshot channel, with a 30-second timeout.
pub struct RealBrowserBridge {
    pub ctx: Arc<BrowserContext>,
}

impl RealBrowserBridge {
    pub fn new(ctx: Arc<BrowserContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl BrowserBridge for RealBrowserBridge {
    async fn call_action(
        &self,
        action: BrowserToolAction,
        params: Value,
        tab_id: Option<i64>,
    ) -> Result<Value, BrowserInputError> {
        let request = BrowserRequest {
            request_id: Uuid::new_v4().to_string(),
            session_id: "browser-input".to_string(),
            tab_id,
            action,
            params,
            timeout_ms: 30_000,
            client_identity: self.ctx.client_identity.clone(),
            proxy_id: self.ctx.proxy_id.clone(),
        };

        let (response_tx, response_rx) = oneshot::channel::<BrowserResponse>();

        self.ctx
            .sender
            .send((request, response_tx))
            .await
            .map_err(|_| BrowserInputError::ChannelClosed)?;

        let response = tokio::time::timeout(Duration::from_secs(30), response_rx)
            .await
            .map_err(|_| BrowserInputError::Timeout { ms: 30_000 })?
            .map_err(|_| BrowserInputError::ChannelClosed)?;

        if response.success {
            Ok(response.result.unwrap_or(Value::Null))
        } else {
            let err = response.error.unwrap_or_else(|| BrowserToolError {
                code: -1,
                message: "Unknown error".into(),
                recoverable: false,
            });
            Err(BrowserInputError::from_browser_error(err))
        }
    }
}

#[cfg(test)]
pub mod testing {
    //! Test-only bridge implementation shared across browser_input tests.
    use super::*;
    use std::sync::Mutex;

    pub struct FakeBridge {
        pub calls: Mutex<Vec<(BrowserToolAction, Value, Option<i64>)>>,
        pub response: Mutex<Result<Value, BrowserInputError>>,
    }

    impl FakeBridge {
        pub fn with_response(response: Result<Value, BrowserInputError>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                response: Mutex::new(response),
            }
        }
    }

    #[async_trait]
    impl BrowserBridge for FakeBridge {
        async fn call_action(
            &self,
            action: BrowserToolAction,
            params: Value,
            tab_id: Option<i64>,
        ) -> Result<Value, BrowserInputError> {
            self.calls.lock().unwrap().push((action, params, tab_id));
            match &*self.response.lock().unwrap() {
                Ok(v) => Ok(v.clone()),
                Err(_) => Err(BrowserInputError::ChannelClosed),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FakeBridge;
    use super::*;

    #[tokio::test]
    async fn fake_bridge_records_calls_and_returns_preset_value() {
        let bridge = FakeBridge::with_response(Ok(serde_json::json!({"ok": true})));

        let result = bridge
            .call_action(
                BrowserToolAction::Probe,
                serde_json::json!({"selector": "#x"}),
                Some(42),
            )
            .await
            .unwrap();

        assert_eq!(result, serde_json::json!({"ok": true}));

        let calls = bridge.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].0, BrowserToolAction::Probe));
        assert_eq!(calls[0].1, serde_json::json!({"selector": "#x"}));
        assert_eq!(calls[0].2, Some(42));
    }

    #[tokio::test]
    async fn fake_bridge_can_return_error() {
        let bridge = FakeBridge::with_response(Err(BrowserInputError::ChannelClosed));
        let r = bridge
            .call_action(BrowserToolAction::Probe, Value::Null, None)
            .await;
        assert!(matches!(r, Err(BrowserInputError::ChannelClosed)));
    }
}
