//! Wire types for the `canvas.persist.*` bridge namespace (My Canvas).
//!
//! Namespace is strictly separate from `canvas.tool.*` (Canvas Tool
//! definitions). All requests/responses are exchanged via the generic
//! `bridge:request` channel.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CanvasPersistSortKey {
    #[default]
    UpdatedAt,
    PersistedAt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CanvasPersistSourceFilter {
    Created,
    Imported,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CanvasPersistListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_filter: Option<CanvasPersistSourceFilter>,
    #[serde(default)]
    pub sort: CanvasPersistSortKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanvasPersistSource {
    Created,
    Imported { share_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPersistSummary {
    pub id: String,
    pub title: String,
    pub content_type: String,
    pub source: CanvasPersistSource,
    pub persisted_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPersistListResponse {
    pub items: Vec<CanvasPersistSummary>,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPersistSaveRequest {
    pub canvas_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPersistSaveResponse {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persisted_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CanvasPersistError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPersistRenameRequest {
    pub canvas_id: String,
    pub new_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPersistRenameResponse {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CanvasPersistError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPersistDeleteRequest {
    pub canvas_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPersistDeleteResponse {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CanvasPersistError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum CanvasPersistError {
    NotFound,
    InvalidTitle { message: String },
    Storage { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_request_defaults_serialize_minimal() {
        let req = CanvasPersistListRequest::default();
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"sort\":\"updated_at\""));
        assert!(!json.contains("search"));
    }

    #[test]
    fn error_round_trip() {
        let e = CanvasPersistError::InvalidTitle {
            message: "empty".into(),
        };
        let j = serde_json::to_string(&e).unwrap();
        let back: CanvasPersistError = serde_json::from_str(&j).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn source_imported_carries_share_id() {
        let s = CanvasPersistSource::Imported {
            share_id: "abc".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"kind\":\"imported\""));
        assert!(j.contains("\"share_id\":\"abc\""));
    }
}
