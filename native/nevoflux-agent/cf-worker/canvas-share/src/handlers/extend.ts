import type { Context } from 'hono';
import type { Env, ShareKVMeta, ExtendRequest } from '../types';
import { isValidShareId, verifyOwnerToken } from '../utils/validation';
import { jsonOk, jsonError, notFound, forbidden } from '../utils/responses';

/**
 * PATCH /api/share/:id
 *
 * Extend the expiration time of a share. Requires the owner token.
 *
 * Request body:
 * - owner_token: base64url-encoded 32-byte token
 * - extend_secs: number of seconds to add
 *
 * Response:
 * - 200: { share_id, expires_at }
 * - 400: Validation error
 * - 403: Invalid owner token
 * - 404: Share not found
 */
export async function handleExtend(c: Context<{ Bindings: Env }>): Promise<Response> {
  const shareId = c.req.param('id');

  if (!shareId || !isValidShareId(shareId)) {
    return jsonError('Invalid share ID format');
  }

  let body: ExtendRequest;
  try {
    body = await c.req.json<ExtendRequest>();
  } catch {
    return jsonError('Invalid JSON body');
  }

  if (!body.owner_token || !body.extend_secs) {
    return jsonError('Missing required fields: owner_token, extend_secs');
  }

  if (body.extend_secs < 60) {
    return jsonError('extend_secs must be at least 60');
  }

  // Fetch current metadata
  const metaStr = await c.env.SHARE_KV.get(`share:${shareId}`);
  if (!metaStr) {
    return notFound('Share not found or expired');
  }

  const meta: ShareKVMeta = JSON.parse(metaStr);

  // Verify owner token
  const isOwner = await verifyOwnerToken(shareId, body.owner_token, meta.owner_token_hash);
  if (!isOwner) {
    return forbidden('Invalid owner token');
  }

  // Check max lifetime constraint
  const maxLifetime = parseInt(c.env.MAX_LIFETIME_SECS, 10);
  const createdAt = new Date(meta.created_at).getTime();
  const currentExpiry = new Date(meta.expires_at).getTime();
  const newExpiry = currentExpiry + body.extend_secs * 1000;
  const totalLifetime = (newExpiry - createdAt) / 1000;

  if (totalLifetime > maxLifetime) {
    return jsonError(
      `Extension would exceed maximum lifetime of ${maxLifetime} seconds. ` +
      `Current lifetime: ${Math.floor((currentExpiry - createdAt) / 1000)}s, ` +
      `remaining budget: ${Math.floor(maxLifetime - (currentExpiry - createdAt) / 1000)}s`
    );
  }

  // Update expiration
  const newExpiresAt = new Date(newExpiry);
  meta.expires_at = newExpiresAt.toISOString();

  const remainingTtl = Math.max(
    60,
    Math.floor((newExpiry - Date.now()) / 1000) + 86400
  );

  await c.env.SHARE_KV.put(`share:${shareId}`, JSON.stringify(meta), {
    expirationTtl: remainingTtl,
  });

  return jsonOk({
    share_id: shareId,
    expires_at: newExpiresAt.toISOString(),
  });
}
