import type { Context } from 'hono';
import type { Env, ShareKVMeta, MetaResponse } from '../types';
import { isValidShareId } from '../utils/validation';
import { jsonOk, jsonError, notFound } from '../utils/responses';

/**
 * GET /api/share/:id/meta
 *
 * Retrieve share metadata without downloading the bundle.
 * Useful for showing share info before the user enters a password.
 *
 * Response:
 * - 200: { share_id, created_at, expires_at, size_bytes, view_count }
 * - 400: Invalid share ID
 * - 404: Share not found or expired
 */
export async function handleMeta(c: Context<{ Bindings: Env }>): Promise<Response> {
  const shareId = c.req.param('id');

  if (!shareId || !isValidShareId(shareId)) {
    return jsonError('Invalid share ID format');
  }

  const metaStr = await c.env.SHARE_KV.get(`share:${shareId}`);
  if (!metaStr) {
    return notFound('Share not found or expired');
  }

  const meta: ShareKVMeta = JSON.parse(metaStr);

  if (new Date(meta.expires_at) < new Date()) {
    return notFound('Share has expired');
  }

  const response: MetaResponse = {
    share_id: shareId,
    created_at: meta.created_at,
    expires_at: meta.expires_at,
    size_bytes: meta.size_bytes,
    view_count: meta.view_count,
  };

  return jsonOk(response);
}
