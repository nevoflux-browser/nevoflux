import type { Context } from 'hono';
import type { Env, ShareKVMeta, DeleteRequest } from '../types';
import { isValidShareId, verifyOwnerToken } from '../utils/validation';
import { jsonOk, jsonError, notFound, forbidden } from '../utils/responses';

/**
 * DELETE /api/share/:id
 *
 * Delete a share. Requires the owner token.
 *
 * Request body:
 * - owner_token: base64url-encoded 32-byte token
 *
 * Response:
 * - 200: { deleted: true }
 * - 400: Validation error
 * - 403: Invalid owner token
 * - 404: Share not found
 */
export async function handleDelete(c: Context<{ Bindings: Env }>): Promise<Response> {
  const shareId = c.req.param('id');

  if (!shareId || !isValidShareId(shareId)) {
    return jsonError('Invalid share ID format');
  }

  let body: DeleteRequest;
  try {
    body = await c.req.json<DeleteRequest>();
  } catch {
    return jsonError('Invalid JSON body');
  }

  if (!body.owner_token) {
    return jsonError('Missing required field: owner_token');
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

  // Delete from R2 and KV in parallel
  await Promise.all([
    c.env.SHARE_BUCKET.delete(`share_assets/${shareId}.bin`),
    c.env.SHARE_KV.delete(`share:${shareId}`),
  ]);

  return jsonOk({ deleted: true });
}
