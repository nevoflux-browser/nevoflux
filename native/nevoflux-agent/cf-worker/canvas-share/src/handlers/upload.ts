import type { Context } from 'hono';
import type { Env, ShareKVMeta, UploadResponse } from '../types';
import { isValidShareId, isValidOwnerTokenHash, hashIp } from '../utils/validation';
import { jsonOk, jsonError, payloadTooLarge } from '../utils/responses';

/**
 * POST /api/share
 *
 * Upload an encrypted share bundle.
 *
 * Request:
 * - Header: Content-Type: application/octet-stream
 * - Query params: share_id, owner_token_hash, expiry_secs (optional)
 * - Body: Raw NFEB binary blob
 *
 * Response:
 * - 201: { share_id, expires_at, size_bytes, url }
 * - 400: Validation error
 * - 409: Share ID already exists
 * - 413: Payload too large
 */
export async function handleUpload(c: Context<{ Bindings: Env }>): Promise<Response> {
  const shareId = c.req.query('share_id');
  const ownerTokenHash = c.req.query('owner_token_hash');
  const expirySecs = parseInt(c.req.query('expiry_secs') || c.env.DEFAULT_EXPIRY_SECS, 10);

  // Validate required params
  if (!shareId || !ownerTokenHash) {
    return jsonError('Missing required query parameters: share_id, owner_token_hash');
  }

  if (!isValidShareId(shareId)) {
    return jsonError('Invalid share_id format (must be 10 chars Crockford base32)');
  }

  if (!isValidOwnerTokenHash(ownerTokenHash)) {
    return jsonError('Invalid owner_token_hash format (must be 64 hex chars)');
  }

  // Validate expiry
  const maxLifetime = parseInt(c.env.MAX_LIFETIME_SECS, 10);
  if (expirySecs < 60 || expirySecs > maxLifetime) {
    return jsonError(`expiry_secs must be between 60 and ${maxLifetime}`);
  }

  // Check if share already exists
  const existing = await c.env.SHARE_KV.get(`share:${shareId}`);
  if (existing) {
    return jsonError('Share ID already exists', 409);
  }

  // Read the binary body
  const maxSize = parseInt(c.env.MAX_BUNDLE_SIZE, 10);
  const body = await c.req.arrayBuffer();

  if (body.byteLength === 0) {
    return jsonError('Empty request body');
  }

  if (body.byteLength > maxSize) {
    return payloadTooLarge(`Bundle size ${body.byteLength} exceeds maximum ${maxSize} bytes`);
  }

  // Validate NFEB magic bytes
  const magic = new Uint8Array(body.slice(0, 4));
  if (
    magic[0] !== 0x4e || // N
    magic[1] !== 0x46 || // F
    magic[2] !== 0x45 || // E
    magic[3] !== 0x42    // B
  ) {
    return jsonError('Invalid bundle format (missing NFEB magic bytes)');
  }

  // Store in R2
  await c.env.SHARE_BUCKET.put(`share_assets/${shareId}.bin`, body, {
    httpMetadata: {
      contentType: 'application/octet-stream',
    },
  });

  // Compute metadata
  const now = new Date();
  const expiresAt = new Date(now.getTime() + expirySecs * 1000);
  const ip = c.req.header('cf-connecting-ip') || c.req.header('x-forwarded-for') || 'unknown';
  const ipHash = await hashIp(ip);

  const meta: ShareKVMeta = {
    created_at: now.toISOString(),
    expires_at: expiresAt.toISOString(),
    size_bytes: body.byteLength,
    owner_token_hash: ownerTokenHash,
    view_count: 0,
    ip_hash: ipHash,
  };

  // Store metadata in KV with TTL
  await c.env.SHARE_KV.put(`share:${shareId}`, JSON.stringify(meta), {
    expirationTtl: expirySecs + 86400, // Add 1 day buffer for cleanup
  });

  const response: UploadResponse = {
    share_id: shareId,
    expires_at: expiresAt.toISOString(),
    size_bytes: body.byteLength,
    url: `https://share.nevoflux.app/c/${shareId}`,
  };

  return jsonOk(response, 201);
}
