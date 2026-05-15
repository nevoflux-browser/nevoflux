/**
 * Cloudflare Worker bindings for Canvas Share.
 */
export interface Env {
  /** R2 bucket for encrypted share bundles. */
  SHARE_BUCKET: R2Bucket;
  /** KV namespace for share metadata. */
  SHARE_KV: KVNamespace;
  /** Maximum bundle size in bytes (default: 57671680 = 55 MiB). */
  MAX_BUNDLE_SIZE: string;
  /** Default expiration in seconds (default: 2592000 = 30 days). */
  DEFAULT_EXPIRY_SECS: string;
  /** Maximum total lifetime in seconds (default: 31536000 = 365 days). */
  MAX_LIFETIME_SECS: string;
  /** Allowed CORS origin pattern. */
  CORS_ORIGIN: string;
}

/**
 * Metadata stored in KV for each share.
 */
export interface ShareKVMeta {
  /** ISO 8601 timestamp when the share was created. */
  created_at: string;
  /** ISO 8601 timestamp when the share expires. */
  expires_at: string;
  /** Size of the encrypted bundle in bytes. */
  size_bytes: number;
  /** SHA-256(share_id || owner_token) for owner authentication. */
  owner_token_hash: string;
  /** Number of times the bundle has been downloaded. */
  view_count: number;
  /** SHA-256 hash of the uploader's IP (for abuse tracking, not PII). */
  ip_hash: string;
}

/**
 * Request body for POST /api/share.
 */
export interface UploadRequest {
  /** 10-char Crockford base32 share ID. */
  share_id: string;
  /** SHA-256(share_id || owner_token) — server stores this hash, not the raw token. */
  owner_token_hash: string;
  /** Expiration time in seconds from now (default: 2592000 = 30 days). */
  expiry_secs?: number;
}

/**
 * Response body for POST /api/share.
 */
export interface UploadResponse {
  share_id: string;
  expires_at: string;
  size_bytes: number;
  url: string;
}

/**
 * Response body for GET /api/share/:id/meta.
 */
export interface MetaResponse {
  share_id: string;
  created_at: string;
  expires_at: string;
  size_bytes: number;
  view_count: number;
}

/**
 * Request body/query for PATCH /api/share/:id.
 */
export interface ExtendRequest {
  /** Raw owner token (base64url-encoded, 32 bytes). */
  owner_token: string;
  /** Additional seconds to extend expiration. */
  extend_secs: number;
}

/**
 * Request body/query for DELETE /api/share/:id.
 */
export interface DeleteRequest {
  /** Raw owner token (base64url-encoded, 32 bytes). */
  owner_token: string;
}
