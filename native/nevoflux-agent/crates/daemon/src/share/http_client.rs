//! HTTP client for the Canvas Share CF Worker API.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{DaemonError, Result};

/// Default base URL for the CF Worker (can be overridden).
pub const DEFAULT_BASE_URL: &str = "https://share.nevoflux.app";

/// HTTP client for the Canvas Share API.
#[derive(Clone)]
pub struct ShareHttpClient {
    base_url: String,
    client: Client,
}

/// Upload response from the CF Worker.
///
/// Matches the deployed CF Worker's `UploadResponse` TypeScript interface:
/// `{ share_id, expires_at (ISO 8601), size_bytes, url }`.
#[derive(Debug, Clone, Deserialize)]
pub struct UploadResponse {
    pub share_id: String,
    /// ISO 8601 timestamp string (not a Unix timestamp).
    pub expires_at: String,
    pub size_bytes: u64,
    pub url: String,
}

/// Metadata response.
///
/// Matches the deployed CF Worker's `MetaResponse` TypeScript interface.
#[derive(Debug, Clone, Deserialize)]
pub struct MetaResponse {
    pub share_id: String,
    /// ISO 8601 timestamp string.
    pub created_at: String,
    /// ISO 8601 timestamp string.
    pub expires_at: String,
    pub size_bytes: u64,
    pub view_count: u64,
}

/// Extend request body.
#[derive(Debug, Clone, Serialize)]
struct ExtendRequestBody<'a> {
    owner_token: &'a str,
    extend_secs: u64,
}

/// Delete request body.
#[derive(Debug, Clone, Serialize)]
struct DeleteRequestBody<'a> {
    owner_token: &'a str,
}

/// Extend response.
#[derive(Debug, Clone, Deserialize)]
pub struct ExtendResponse {
    pub share_id: String,
    /// ISO 8601 timestamp string.
    pub expires_at: String,
}

/// Delete response.
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteResponse {
    pub deleted: bool,
}

impl ShareHttpClient {
    /// Create a new HTTP client pointing at the given base URL.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| DaemonError::InternalError(format!("reqwest build error: {}", e)))?;
        Ok(Self {
            base_url: base_url.into(),
            client,
        })
    }

    /// Create a client pointing at the default CF Worker URL.
    pub fn with_default_url() -> Result<Self> {
        Self::new(DEFAULT_BASE_URL)
    }

    /// Access the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Upload an encrypted NFEB bundle.
    ///
    /// - `share_id`: 10-char Crockford base32 share ID (caller-generated)
    /// - `nfeb_bytes`: serialized encrypted bundle
    /// - `owner_token_hash`: hex-encoded SHA-256 hash of the owner token (for later auth)
    /// - `ttl_secs`: requested TTL in seconds (server may cap)
    pub async fn upload(
        &self,
        share_id: &str,
        nfeb_bytes: &[u8],
        owner_token_hash: &str,
        ttl_secs: u64,
    ) -> Result<UploadResponse> {
        let url = format!(
            "{}/api/share?share_id={}&owner_token_hash={}&expiry_secs={}",
            self.base_url, share_id, owner_token_hash, ttl_secs,
        );
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(nfeb_bytes.to_vec())
            .send()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Upload request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::InternalError(format!(
                "Upload failed: {} - {}",
                status, body
            )));
        }

        response
            .json::<UploadResponse>()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Upload response parse: {}", e)))
    }

    /// Download a NFEB bundle by share ID.
    pub async fn fetch_bundle(&self, share_id: &str) -> Result<Vec<u8>> {
        let url = format!("{}/api/share/{}/bundle", self.base_url, share_id);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Fetch request failed: {}", e)))?;

        if response.status() == 404 {
            return Err(DaemonError::InvalidRequest(format!(
                "Share not found: {}",
                share_id
            )));
        }
        if !response.status().is_success() {
            let status = response.status();
            return Err(DaemonError::InternalError(format!(
                "Fetch failed: {}",
                status
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Read body: {}", e)))?;
        Ok(bytes.to_vec())
    }

    /// Fetch metadata only (no bundle download).
    pub async fn fetch_meta(&self, share_id: &str) -> Result<MetaResponse> {
        let url = format!("{}/api/share/{}/meta", self.base_url, share_id);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Meta request failed: {}", e)))?;

        if response.status() == 404 {
            return Err(DaemonError::InvalidRequest(format!(
                "Share not found: {}",
                share_id
            )));
        }
        if !response.status().is_success() {
            return Err(DaemonError::InternalError(format!(
                "Meta failed: {}",
                response.status()
            )));
        }

        response
            .json::<MetaResponse>()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Meta parse: {}", e)))
    }

    /// Extend share TTL.
    ///
    /// `owner_token_b64url` must be the base64url-encoded (no padding) 32-byte
    /// owner token, as expected by the CF Worker.
    pub async fn extend(
        &self,
        share_id: &str,
        owner_token_b64url: &str,
        extend_secs: u64,
    ) -> Result<ExtendResponse> {
        let url = format!("{}/api/share/{}", self.base_url, share_id);
        let body = ExtendRequestBody {
            owner_token: owner_token_b64url,
            extend_secs,
        };
        let response = self
            .client
            .patch(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Extend request failed: {}", e)))?;

        if response.status() == 403 {
            return Err(DaemonError::InvalidRequest(
                "Unauthorized (wrong owner token)".into(),
            ));
        }
        if response.status() == 404 {
            return Err(DaemonError::InvalidRequest(format!(
                "Share not found: {}",
                share_id
            )));
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::InternalError(format!(
                "Extend failed: {} - {}",
                status, body
            )));
        }

        response
            .json::<ExtendResponse>()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Extend parse: {}", e)))
    }

    /// Delete a share.
    ///
    /// `owner_token_b64url` must be the base64url-encoded (no padding) 32-byte
    /// owner token, as expected by the CF Worker.
    pub async fn delete(&self, share_id: &str, owner_token_b64url: &str) -> Result<()> {
        let url = format!("{}/api/share/{}", self.base_url, share_id);
        let body = DeleteRequestBody {
            owner_token: owner_token_b64url,
        };
        let response = self
            .client
            .delete(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Delete request failed: {}", e)))?;

        if response.status() == 403 {
            return Err(DaemonError::InvalidRequest("Unauthorized".into()));
        }
        if response.status() == 404 {
            return Err(DaemonError::InvalidRequest(format!(
                "Share not found: {}",
                share_id
            )));
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::InternalError(format!(
                "Delete failed: {} - {}",
                status, body
            )));
        }

        // The CF Worker returns `{ deleted: true }` with 200. Parse and verify.
        let resp = response
            .json::<DeleteResponse>()
            .await
            .map_err(|e| DaemonError::InternalError(format!("Delete parse: {}", e)))?;
        if !resp.deleted {
            return Err(DaemonError::InternalError(
                "Server reported delete did not succeed".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_construction() {
        let client = ShareHttpClient::new("https://example.com").unwrap();
        assert_eq!(client.base_url(), "https://example.com");
    }

    #[test]
    fn test_client_default_url() {
        let client = ShareHttpClient::with_default_url().unwrap();
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_is_clone() {
        let client = ShareHttpClient::new("https://example.com").unwrap();
        let cloned = client.clone();
        assert_eq!(cloned.base_url(), "https://example.com");
    }

    #[test]
    fn test_extend_request_body_serialization() {
        let body = ExtendRequestBody {
            owner_token: "abc",
            extend_secs: 3600,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert_eq!(json, r#"{"owner_token":"abc","extend_secs":3600}"#);
    }

    #[test]
    fn test_delete_request_body_serialization() {
        let body = DeleteRequestBody { owner_token: "abc" };
        let json = serde_json::to_string(&body).unwrap();
        assert_eq!(json, r#"{"owner_token":"abc"}"#);
    }

    #[test]
    fn test_upload_response_deserialization() {
        let json = r#"{"share_id":"abc123","expires_at":"2026-04-13T12:00:00.000Z","size_bytes":1024,"url":"https://share.nevoflux.app/c/abc123"}"#;
        let resp: UploadResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.share_id, "abc123");
        assert_eq!(resp.expires_at, "2026-04-13T12:00:00.000Z");
        assert_eq!(resp.size_bytes, 1024);
        assert_eq!(resp.url, "https://share.nevoflux.app/c/abc123");
    }

    #[test]
    fn test_meta_response_deserialization() {
        let json = r#"{"share_id":"abc123","created_at":"2026-04-13T10:00:00.000Z","expires_at":"2026-04-13T12:00:00.000Z","size_bytes":1024,"view_count":42}"#;
        let resp: MetaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.share_id, "abc123");
        assert_eq!(resp.created_at, "2026-04-13T10:00:00.000Z");
        assert_eq!(resp.expires_at, "2026-04-13T12:00:00.000Z");
        assert_eq!(resp.view_count, 42);
        assert_eq!(resp.size_bytes, 1024);
    }

    #[test]
    fn test_extend_response_deserialization() {
        let json = r#"{"share_id":"abc123","expires_at":"2026-04-13T12:00:00.000Z"}"#;
        let resp: ExtendResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.share_id, "abc123");
        assert_eq!(resp.expires_at, "2026-04-13T12:00:00.000Z");
    }

    #[test]
    fn test_delete_response_deserialization() {
        let json = r#"{"deleted":true}"#;
        let resp: DeleteResponse = serde_json::from_str(json).unwrap();
        assert!(resp.deleted);
    }

    /// Network-hitting smoke test; ignored by default.
    #[tokio::test]
    #[ignore]
    async fn test_fetch_bundle_not_found() {
        let client = ShareHttpClient::with_default_url().unwrap();
        let result = client.fetch_bundle("nonexistent-share-id-12345").await;
        assert!(result.is_err());
    }
}
