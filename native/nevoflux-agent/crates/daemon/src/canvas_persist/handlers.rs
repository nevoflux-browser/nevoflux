//! Bridge dispatch for `canvas.persist.*` messages.
//!
//! Each handler parses the typed request out of a `serde_json::Value`,
//! invokes the corresponding `CanvasPersistService` method, and returns
//! the response as a `serde_json::Value` for the bridge layer to wrap.

use std::sync::Arc;

use nevoflux_protocol::canvas_persist::{
    CanvasPersistDeleteRequest, CanvasPersistListRequest, CanvasPersistRenameRequest,
    CanvasPersistSaveRequest,
};
use serde_json::Value;

use crate::canvas_persist::CanvasPersistService;
use crate::error::{DaemonError, Result};

/// Dispatch a `canvas.persist.*` message to the appropriate service call.
///
/// Returns the JSON-encoded response payload on success. Unknown types
/// resolve to `DaemonError::InvalidRequest` (so the bridge can surface a clean
/// "unknown command" error).
pub fn handle(
    svc: &Arc<CanvasPersistService>,
    message_type: &str,
    payload: Value,
) -> Result<Value> {
    match message_type {
        "canvas_persist_list" => {
            let req: CanvasPersistListRequest = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("canvas_persist_list: {e}")))?;
            serde_json::to_value(svc.list(req)?)
                .map_err(|e| DaemonError::SerializationError(format!("encode list response: {e}")))
        }
        "canvas_persist_save" => {
            let req: CanvasPersistSaveRequest = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("canvas_persist_save: {e}")))?;
            serde_json::to_value(svc.save(req)?)
                .map_err(|e| DaemonError::SerializationError(format!("encode save response: {e}")))
        }
        "canvas_persist_rename" => {
            let req: CanvasPersistRenameRequest = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("canvas_persist_rename: {e}")))?;
            serde_json::to_value(svc.rename(req)?).map_err(|e| {
                DaemonError::SerializationError(format!("encode rename response: {e}"))
            })
        }
        "canvas_persist_delete" => {
            let req: CanvasPersistDeleteRequest = serde_json::from_value(payload)
                .map_err(|e| DaemonError::InvalidRequest(format!("canvas_persist_delete: {e}")))?;
            serde_json::to_value(svc.delete(req)?).map_err(|e| {
                DaemonError::SerializationError(format!("encode delete response: {e}"))
            })
        }
        other => Err(DaemonError::InvalidRequest(format!(
            "unknown canvas.persist.* message type: {other}"
        ))),
    }
}
