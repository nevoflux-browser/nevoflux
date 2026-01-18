/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Channel 4: Page Mode LLM handler (bidirectional)

use anyhow::Result;
use shared_protocol::{PageLlmDonePayload, PageLlmErrorPayload, PageLlmMessage, PageLlmRequestPayload};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, warn};

/// Handler for Channel 4 (Page Mode LLM)
pub struct PageLlmChannelHandler {
    outgoing: mpsc::UnboundedSender<String>,
    pending_requests: Arc<RwLock<HashMap<String, PageLlmRequestState>>>,
}

struct PageLlmRequestState {
    chunks_tx: mpsc::UnboundedSender<String>,
    done_tx: oneshot::Sender<Result<PageLlmDonePayload, PageLlmErrorPayload>>,
}

impl PageLlmChannelHandler {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<String>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Self {
                outgoing: sender,
                pending_requests: Arc::new(RwLock::new(HashMap::new())),
            },
            receiver,
        )
    }

    /// Handle incoming Page LLM message
    pub async fn handle_message(&self, json: &str) -> Result<()> {
        let message: PageLlmMessage = serde_json::from_str(json)?;
        debug!("Page LLM channel received message");

        match message {
            PageLlmMessage::PageLlmChunk(payload) => {
                let pending = self.pending_requests.read().await;
                if let Some(state) = pending.get(&payload.request_id) {
                    if let Some(choice) = payload.payload.choices.first() {
                        if let Some(content) = &choice.delta.content {
                            let _ = state.chunks_tx.send(content.clone());
                        }
                    }
                }
            }
            PageLlmMessage::PageLlmDone(payload) => {
                info!("Page LLM done: request={}", payload.request_id);
                let mut pending = self.pending_requests.write().await;
                if let Some(state) = pending.remove(&payload.request_id) {
                    let _ = state.done_tx.send(Ok(payload));
                }
            }
            PageLlmMessage::PageLlmError(payload) => {
                warn!(
                    "Page LLM error: request={}, code={}",
                    payload.request_id, payload.error.code
                );
                let mut pending = self.pending_requests.write().await;
                if let Some(state) = pending.remove(&payload.request_id) {
                    let _ = state.done_tx.send(Err(payload));
                }
            }
            PageLlmMessage::PageLlmRequest(_) => {
                debug!("Unexpected Page LLM request received");
            }
        }
        Ok(())
    }

    /// Send Page LLM request
    pub async fn send_request(
        &self,
        request: PageLlmRequestPayload,
    ) -> Result<(
        mpsc::UnboundedReceiver<String>,
        oneshot::Receiver<Result<PageLlmDonePayload, PageLlmErrorPayload>>,
    )> {
        let request_id = request.request_id.clone();
        let (chunks_tx, chunks_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(request_id.clone(), PageLlmRequestState { chunks_tx, done_tx });
        }

        let msg = PageLlmMessage::PageLlmRequest(request);
        let json = serde_json::to_string(&msg)?;
        self.outgoing.send(json)?;

        Ok((chunks_rx, done_rx))
    }
}

impl Default for PageLlmChannelHandler {
    fn default() -> Self {
        Self::new().0
    }
}
