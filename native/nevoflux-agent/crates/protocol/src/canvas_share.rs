//! Canvas Share protocol message types.
//!
//! These types define the wire protocol for sharing and importing canvas
//! artifacts between clients. A share encrypts an artifact with a password
//! and uploads it to a relay; the resulting share URL + password can be
//! used by another client to import the artifact.
//!
//! # Security
//!
//! The password returned by [`CanvasShareResponse`] is only returned once.
//! Callers must surface the password to the user immediately. There is no
//! mechanism to recover the password after the response is consumed.

use serde::{Deserialize, Serialize};

/// Request to share a canvas (encrypt + upload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasShareRequest {
    pub session_id: String,
    pub artifact_id: String,
    /// Optional TTL in seconds (default 30 days = 2592000).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
}

/// Response containing the share URL + password.
/// **SECURITY:** The password is only returned once. Caller must show to user immediately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasShareResponse {
    pub share_id: String,
    pub share_url: String,
    pub password: String,
    pub expires_at: i64,
}

/// Request to import a shared canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasImportRequest {
    pub session_id: String,
    pub share_id: String,
    pub password: String,
}

/// Response from import with the decrypted artifact info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasImportResponse {
    pub artifact_id: String,
    pub artifact_name: String,
    pub artifact_type: String,
    pub imported_from_share_id: String,
}

/// Request to extend a share's TTL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasShareExtendRequest {
    pub share_id: String,
    pub extend_secs: u64,
}

/// Response with new expiry time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasShareExtendResponse {
    pub share_id: String,
    pub expires_at: i64,
}

/// Request to delete a share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasShareDeleteRequest {
    pub share_id: String,
}

/// Response confirming delete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasShareDeleteResponse {
    pub share_id: String,
    pub success: bool,
}

/// Request to list all active shares for a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasShareListRequest {
    pub session_id: String,
}

/// A single active share entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasShareInfo {
    pub artifact_id: String,
    pub share_id: String,
    pub share_url: String,
    pub expires_at: i64,
    pub view_count: u64,
    pub created_at: i64,
}

/// Response listing shares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasShareListResponse {
    pub shares: Vec<CanvasShareInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_share_request_roundtrip_with_ttl() {
        let req = CanvasShareRequest {
            session_id: "sess-1".to_string(),
            artifact_id: "art-1".to_string(),
            ttl_secs: Some(3600),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"ttl_secs\":3600"));
        let decoded: CanvasShareRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn canvas_share_request_roundtrip_without_ttl() {
        let req = CanvasShareRequest {
            session_id: "sess-2".to_string(),
            artifact_id: "art-2".to_string(),
            ttl_secs: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        // `ttl_secs` should be skipped when None.
        assert!(!json.contains("ttl_secs"));
        let decoded: CanvasShareRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn canvas_share_response_roundtrip() {
        let resp = CanvasShareResponse {
            share_id: "sh-123".to_string(),
            share_url: "https://relay.example/s/sh-123".to_string(),
            password: "correct-horse-battery-staple".to_string(),
            expires_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CanvasShareResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn canvas_import_request_roundtrip() {
        let req = CanvasImportRequest {
            session_id: "sess-9".to_string(),
            share_id: "sh-9".to_string(),
            password: "p@ssw0rd!".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: CanvasImportRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn canvas_import_response_roundtrip() {
        let resp = CanvasImportResponse {
            artifact_id: "art-new".to_string(),
            artifact_name: "design.svg".to_string(),
            artifact_type: "image/svg+xml".to_string(),
            imported_from_share_id: "sh-9".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CanvasImportResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn canvas_share_extend_roundtrip() {
        let req = CanvasShareExtendRequest {
            share_id: "sh-1".to_string(),
            extend_secs: 86_400,
        };
        let resp = CanvasShareExtendResponse {
            share_id: "sh-1".to_string(),
            expires_at: 1_800_000_000,
        };
        let req_json = serde_json::to_string(&req).unwrap();
        let resp_json = serde_json::to_string(&resp).unwrap();
        assert_eq!(
            req,
            serde_json::from_str::<CanvasShareExtendRequest>(&req_json).unwrap()
        );
        assert_eq!(
            resp,
            serde_json::from_str::<CanvasShareExtendResponse>(&resp_json).unwrap()
        );
    }

    #[test]
    fn canvas_share_delete_roundtrip() {
        let req = CanvasShareDeleteRequest {
            share_id: "sh-del".to_string(),
        };
        let resp = CanvasShareDeleteResponse {
            share_id: "sh-del".to_string(),
            success: true,
        };
        let req_json = serde_json::to_string(&req).unwrap();
        let resp_json = serde_json::to_string(&resp).unwrap();
        assert_eq!(
            req,
            serde_json::from_str::<CanvasShareDeleteRequest>(&req_json).unwrap()
        );
        assert_eq!(
            resp,
            serde_json::from_str::<CanvasShareDeleteResponse>(&resp_json).unwrap()
        );
    }

    #[test]
    fn canvas_share_list_roundtrip() {
        let req = CanvasShareListRequest {
            session_id: "sess-L".to_string(),
        };
        let resp = CanvasShareListResponse {
            shares: vec![
                CanvasShareInfo {
                    artifact_id: "art-1".to_string(),
                    share_id: "sh-1".to_string(),
                    share_url: "https://relay.example/s/sh-1".to_string(),
                    expires_at: 1_700_000_000,
                    view_count: 2,
                    created_at: 1_699_000_000,
                },
                CanvasShareInfo {
                    artifact_id: "art-2".to_string(),
                    share_id: "sh-2".to_string(),
                    share_url: "https://relay.example/s/sh-2".to_string(),
                    expires_at: 1_700_500_000,
                    view_count: 0,
                    created_at: 1_699_500_000,
                },
            ],
        };
        let req_json = serde_json::to_string(&req).unwrap();
        let resp_json = serde_json::to_string(&resp).unwrap();
        assert_eq!(
            req,
            serde_json::from_str::<CanvasShareListRequest>(&req_json).unwrap()
        );
        assert_eq!(
            resp,
            serde_json::from_str::<CanvasShareListResponse>(&resp_json).unwrap()
        );
    }

    #[test]
    fn canvas_share_list_empty_roundtrip() {
        let resp = CanvasShareListResponse { shares: vec![] };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CanvasShareListResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
        assert!(decoded.shares.is_empty());
    }
}
